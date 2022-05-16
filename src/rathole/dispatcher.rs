use std::io::Cursor;
use std::sync::Arc;

use bytes::{Buf, BytesMut};
use log::info;
use tokio::io;
use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter, ReadHalf, WriteHalf,
};
use tokio::sync::mpsc;

use crate::rathole::cmd::Command;
use crate::rathole::context::{CommandSender, Context};
use crate::rathole::frame::Frame;
use crate::rathole::CommandChannel;

pub struct Dispatcher<T> {
    command_read: CommandRead<T>,
    command_write: CommandWrite<T>,

    context: Context,
    receiver: mpsc::Receiver<CommandChannel>,
}

/// command read
pub struct CommandRead<T> {
    read: ReadHalf<T>,
    buffer: BytesMut,
}

/// command write
pub struct CommandWrite<T> {
    write: BufWriter<WriteHalf<T>>,
}

impl<T: AsyncRead + AsyncWrite + Unpin> Dispatcher<T> {
    pub fn new(stream: T, hash: String) -> Self {
        let (sender, receiver) = mpsc::channel(128);
        let command_sender = Arc::new(CommandSender::new(hash, sender));
        let context = Context::new(command_sender);
        let (read, write) = io::split(stream);
        let command_read = CommandRead::new(read);
        let command_write = CommandWrite::new(write);

        Dispatcher {
            context,
            command_read,
            command_write,
            receiver,
        }
    }

    pub async fn dispatch(&mut self) -> crate::Result<()> {
        loop {
            tokio::select! {
                r1 = Self::apply_command(
                    &mut self.command_read,
                    self.context.clone(),
                ) => {
                    r1?
                },
                r2 = Self::recv_command(
                    &mut self.command_write,
                    &mut self.receiver,
                    self.context.clone(),
                ) => {
                    r2?
                },
            }
        }
    }

    async fn apply_command(
        command_read: &mut CommandRead<T>,
        context: Context,
    ) -> crate::Result<()> {
        let mut nc = context.clone();

        let (req_id, cmd) = command_read.read_command().await?;

        nc.with_req_id(req_id);
        let op_resp = cmd.apply(nc).await?;

        if let Some(resp) = op_resp {
            context.command_sender.send_with_id(req_id, resp).await?
        }
        Ok(())
    }

    async fn recv_command(
        command_write: &mut CommandWrite<T>,
        receiver: &mut mpsc::Receiver<CommandChannel>,
        mut context: Context,
    ) -> crate::Result<()> {
        let oc = receiver.recv().await;
        match oc {
            Some((req_id, cmd, rc)) => {
                context.with_req_id(req_id);
                context.set_req(rc).await;
                command_write.write_command(req_id, cmd).await
            }
            None => Err("cmd receiver close".into()),
        }
    }

    pub fn get_command_sender(&self) -> Arc<CommandSender> {
        self.context.command_sender.clone()
    }
}

impl<T: AsyncRead> CommandRead<T> {
    pub fn new(read: ReadHalf<T>) -> Self {
        CommandRead {
            read,
            buffer: BytesMut::new(),
        }
    }

    pub async fn read_command(&mut self) -> crate::Result<(u64, Command)> {
        let f = self.read_frame().await?;
        info!("read frame : {}", f);
        let (req_id, cmd) = Command::from_frame(f)?;
        Ok((req_id, cmd))
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
    pub fn new(write: WriteHalf<T>) -> Self {
        CommandWrite {
            write: BufWriter::new(write),
        }
    }

    pub async fn write_command(&mut self, req_id: u64, cmd: Command) -> crate::Result<()> {
        let frame = cmd.to_frame(req_id)?;

        info!("write frame : {}", frame);
        self.write_frame(&frame).await?;
        Ok(())
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

    use bytes::Bytes;

    use crate::rathole::frame::Frame;
    use crate::rathole::tests::{new_command_read, new_command_write};

    async fn frame_write_read(frame: &Frame) -> Frame {
        let mut cell = RefCell::new(Vec::new());
        let mut command_write = new_command_write(cell.get_mut());
        command_write.write_frame(&frame).await.unwrap();

        let mut command_read = new_command_read(cell.get_mut());
        let f = command_read.read_frame().await.unwrap();
        f
    }

    #[tokio::test]
    async fn test_read_write_frame_simple() {
        let frame = Frame::Simple("hello".to_string());
        let f = frame_write_read(&frame).await;
        assert!(f.eq(&"hello"))
    }

    #[tokio::test]
    async fn test_read_write_frame_array() {
        let frame = Frame::Array(vec![
            Frame::Simple("hello".to_string()),
            Frame::Integer(2),
            Frame::Bulk(Bytes::from(vec![0x01, 0x02])),
        ]);
        let f = frame_write_read(&frame).await;
        assert_eq!(format!("{:?}", frame), format!("{:?}", f))
    }
}
