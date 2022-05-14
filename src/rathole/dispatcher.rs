use std::io::Cursor;
use std::sync::Arc;

use bytes::{Buf, Bytes, BytesMut};
use log::info;
use tokio::io;
use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter, ReadHalf, WriteHalf,
};
use tokio::sync::mpsc;

use crate::rathole::cmd::Command;
use crate::rathole::frame::Frame;
use crate::store::ServerStore;

pub struct Dispatcher<T> {
    command_read: CommandRead<T>,
    command_write: CommandWrite<T>,

    command_sender: Arc<CommandSender>,
    receiver: mpsc::Receiver<Command>,

    store: ServerStore,
    req_id_adder: ReqIdAdder,
}

/// command sender
#[derive(Debug, Clone)]
pub struct CommandSender {
    pub hash: String,
    pub sender: mpsc::Sender<Command>,
}

#[derive(Debug, Clone)]
pub struct ConnSender {
    pub conn_id: String,
    pub sender: mpsc::Sender<Bytes>,
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
    pub fn new(stream: T, hash: String, store: ServerStore) -> Self {
        let (sender, receiver) = mpsc::channel(128);
        let command_sender = Arc::new(CommandSender::new(hash, sender));
        let (read, write) = io::split(stream);
        let command_read = CommandRead::new(read);
        let command_write = CommandWrite::new(write);

        Dispatcher {
            command_sender,
            command_read,
            command_write,
            receiver,
            store,
            req_id_adder: ReqIdAdder::new(),
        }
    }

    pub async fn dispatch(&mut self) -> crate::Result<()> {
        loop {
            tokio::select! {
                r1 = Self::apply_command(
                    &mut self.command_read,
                    self.command_sender.clone(),
                    self.store.clone()
                ) => {
                    r1?
                },
                r2 = Self::recv_command(
                    &mut self.command_write,
                    &mut self.receiver,
                    &mut self.req_id_adder,
                ) => {
                    r2?
                },
            }
        }
    }

    async fn apply_command(
        command_read: &mut CommandRead<T>,
        command_sender: Arc<CommandSender>,
        store: ServerStore,
    ) -> crate::Result<()> {
        let (req_id, cmd) = command_read.read_command().await?;
        let resp = cmd.apply(store)?;
        if resp.is_some() {
            let resp_id = if let Some(Command::Resp(resp)) = resp {
                Command::RespId(req_id, resp)
            } else {
                panic!("expect resp command");
            };
            command_sender.send(resp_id).await?
        }
        Ok(())
    }

    async fn recv_command(
        command_write: &mut CommandWrite<T>,
        receiver: &mut mpsc::Receiver<Command>,
        req_id_adder: &mut ReqIdAdder,
    ) -> crate::Result<()> {
        let oc = receiver.recv().await;
        match oc {
            Some(cmd) => {
                let req_id = req_id_adder.get_req_id();
                command_write.write_command(req_id, cmd).await
            }
            None => Err("cmd receiver close".into()),
        }
    }

    pub fn get_command_sender(&self) -> Arc<CommandSender> {
        self.command_sender.clone()
    }
}

impl CommandSender {
    pub fn new(hash: String, sender: mpsc::Sender<Command>) -> Self {
        CommandSender { hash, sender }
    }

    pub async fn send(&self, cmd: Command) -> crate::Result<()> {
        Ok(self.sender.send(cmd).await?)
    }
}

impl ConnSender {
    pub fn new(conn_id: String, sender: mpsc::Sender<Bytes>) -> Self {
        ConnSender { conn_id, sender }
    }

    pub async fn send(&self, byte: Bytes) -> crate::Result<()> {
        Ok(self.sender.send(byte).await?)
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
        info!("frame : {}", f);
        let (req_id, cmd) = Command::from_frame(f)?;
        info!("cmd : {:?}", cmd);
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
        info!("write cmd :{:?}", cmd);
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

struct ReqIdAdder {
    id_adder: u64,
}

impl ReqIdAdder {
    pub fn new() -> Self {
        ReqIdAdder { id_adder: 0 }
    }

    pub fn get_req_id(&mut self) -> u64 {
        self.id_adder += 1;
        self.id_adder
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Cursor;

    use bytes::Bytes;

    use crate::common::consts;
    use crate::rathole::dispatcher::{CommandRead, CommandWrite};
    use crate::rathole::frame::Frame;

    fn new_command_read(buf: Vec<u8>) -> CommandRead<Cursor<Vec<u8>>> {
        let (r, _w) = tokio::io::split(Cursor::new(buf));
        CommandRead::new(r)
    }

    fn new_command_write(buf: &mut Vec<u8>) -> CommandWrite<Cursor<&mut Vec<u8>>> {
        let (_r, w) = tokio::io::split(Cursor::new(buf));
        CommandWrite::new(w)
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
        let frame = Frame::Array(vec![
            Frame::Simple("hello".to_string()),
            Frame::Integer(2),
            Frame::Bulk(Bytes::from(vec![0x01, 0x02])),
        ]);

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
