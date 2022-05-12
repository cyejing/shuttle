use std::io::Cursor;
use std::sync::Arc;

use bytes::{Buf, BytesMut};
use log::{info};
use tokio::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter, ReadHalf, WriteHalf};
use tokio::sync::{mpsc};

use crate::rathole::cmd::Command;
use crate::rathole::frame::Frame;

/// command sender
#[derive(Debug, Clone)]
pub struct CmdSender {
    pub hash: String,
    pub sender: mpsc::Sender<Command>,
}

impl CmdSender {
    pub async fn send(&self, cmd: Command) -> crate::Result<()> {
        Ok(self.sender.send(cmd).await?)
    }
}

/// command read
pub struct CommandRead<T> {
    read: ReadHalf<T>,
    buffer: BytesMut,
    sender: Arc<CmdSender>,
}

/// command write
pub struct CommandWrite<T> {
    write: BufWriter<WriteHalf<T>>,
    receiver: mpsc::Receiver<Command>,
}

impl<T: AsyncRead> CommandRead<T> {
    pub fn new(read: ReadHalf<T>, sender: Arc<CmdSender>) -> Self {
        CommandRead {
            read,
            sender,
            buffer: BytesMut::new(),
        }
    }

    pub async fn read_command(&mut self) -> crate::Result<()> {
        let f = self.read_frame().await?;
        info!("frame : {}", f);
        let cmd = Command::from_frame(f)?;
        info!("cmd : {:?}", cmd);
        cmd.apply(self.sender.clone()).await?;
        Ok(())
    }

    async fn read_frame(&mut self) -> crate::Result<Frame> {
        loop {
            if let Some(frame) = self.parse_frame()? {
                return Ok(frame);
            }

            if 0 == self.read.read_buf(&mut self.buffer).await? {
                return Err("connection reset by peer".into());
            } else {
                info!("{}", pretty_hex::pretty_hex(&self.buffer))
            }
        }
    }

    fn parse_frame(&mut self) -> crate::Result<Option<Frame>> {
        use crate::rathole::frame::Error::Incomplete;

        let mut buf = Cursor::new(&self.buffer[..]);

        match Frame::check(&mut buf) {
            Ok(_) => {
                let len = buf.position() as usize;

                buf.set_position(0);

                let frame = Frame::parse(&mut buf)?;

                self.buffer.advance(len);

                Ok(Some(frame))
            }

            Err(Incomplete) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}


impl<T: AsyncWrite> CommandWrite<T> {
    pub fn new(write: WriteHalf<T>, receiver: mpsc::Receiver<Command>) -> Self {
        CommandWrite {
            write: BufWriter::new(write),
            receiver,
        }
    }

    pub async fn write_command(&mut self) -> crate::Result<()> {
        let oc = self.receiver.recv().await;
        match oc {
            Some(cmd) => {
                info!("recv cmd :{:?}", cmd);
                let frame = Command::exec(cmd)?;
                info!("write frame : {}", frame);
                self.write_frame(&frame).await?;
                Ok(())
            }
            None => Err("cmd receiver close".into()),
        }
    }

    async fn write_frame(&mut self, frame: &Frame) -> io::Result<()> {
        match frame {
            Frame::Array(val) => {
                self.write.write_u8(b'*').await?;

                self.write_decimal(val.len() as u64).await?;

                for entry in &**val {
                    self.write_value(entry).await?;
                }
            }
            _ => self.write_value(frame).await?,
        }

        self.write.flush().await
    }


    /// Write a frame literal to the stream
    async fn write_value(&mut self, frame: &Frame) -> io::Result<()> {
        match frame {
            Frame::Simple(val) => {
                self.write.write_u8(b'+').await?;
                self.write.write_all(val.as_bytes()).await?;
                self.write.write_all(b"\r\n").await?;
            }
            Frame::Error(val) => {
                self.write.write_u8(b'-').await?;
                self.write.write_all(val.as_bytes()).await?;
                self.write.write_all(b"\r\n").await?;
            }
            Frame::Integer(val) => {
                self.write.write_u8(b':').await?;
                self.write_decimal(*val).await?;
            }
            Frame::Null => {
                self.write.write_all(b"$-1\r\n").await?;
            }
            Frame::Bulk(val) => {
                let len = val.len();

                self.write.write_u8(b'$').await?;
                self.write_decimal(len as u64).await?;
                self.write.write_all(val).await?;
                self.write.write_all(b"\r\n").await?;
            }
            Frame::Array(_val) => unreachable!(),
        }

        Ok(())
    }

    /// Write a decimal frame to the stream
    async fn write_decimal(&mut self, val: u64) -> io::Result<()> {
        use std::io::Write;

        let mut buf = [0u8; 20];
        let mut buf = Cursor::new(&mut buf[..]);
        write!(&mut buf, "{}", val)?;

        let pos = buf.position() as usize;
        self.write.write_all(&buf.get_ref()[..pos]).await?;
        self.write.write_all(b"\r\n").await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::{Cursor};
    use std::sync::Arc;

    use bytes::Bytes;
    use tokio::sync::mpsc;

    use crate::common::consts;
    use crate::rathole::session::{CmdSender, CommandRead, CommandWrite};
    use crate::rathole::frame::Frame;

    fn new_command_read(buf: Vec<u8>) -> CommandRead<Cursor<Vec<u8>>> {
        let (sender, _receiver) = mpsc::channel(128);
        let hash = String::from("hash");
        let cmd_sender = Arc::new(CmdSender { hash, sender });
        let (r,_w) = tokio::io::split(Cursor::new(buf));
        CommandRead::new(r, cmd_sender)
    }

    fn new_command_write(buf: &mut Vec<u8>) -> CommandWrite<Cursor<&mut Vec<u8>>>{
        let (_sender, receiver) = mpsc::channel(128);
        let (_r,w) = tokio::io::split(Cursor::new(buf));
        CommandWrite::new(w, receiver)
    }

    #[tokio::test]
    async fn test_read_write_frame_simple() {
        let frame = Frame::Simple("hello".to_string());

        let mut cell = RefCell::new(Vec::new());
        let mut command_write = new_command_write(cell.get_mut());
        command_write.write_frame(&frame).await.unwrap();
        let read = cell.get_mut().as_slice().clone();
        println!("{:?}", read);

        let mut command_read = new_command_read(Vec::from(read));
        let f = command_read.read_frame().await.unwrap();
        println!("{:?}", f);
        assert!(frame.eq(&"hello"))
    }

    #[tokio::test]
    async fn test_read_write_frame_array() {
        let frame = Frame::Array(vec![Frame::Simple("hello".to_string()),
                                      Frame::Integer(2),
                                      Frame::Bulk(Bytes::from(vec![0x01, 0x02]))]);

        let mut cell = RefCell::new(Vec::new());
        let mut command_write = new_command_write(cell.get_mut());
        command_write.write_frame(&frame).await.unwrap();
        let read = cell.get_mut().as_slice().clone();
        println!("{:?}", read);

        let mut command_read = new_command_read(Vec::from(read));
        let f = command_read.read_frame().await.unwrap();
        println!("{:?}", f);

        assert_eq!(format!("{:?}", frame), format!("{:?}", f))
    }


    #[tokio::test]
    async fn test_read_frame() {
        let mut bs: Vec<u8> = Vec::new();
        bs.extend_from_slice("*3".as_bytes());
        bs.extend_from_slice(consts::CRLF.as_slice());
        bs.extend_from_slice("*3".as_bytes());
    }
}
