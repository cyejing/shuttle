use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use bytes::{Buf, BytesMut};
use tokio::io;
use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter, ReadHalf, WriteHalf,
};
use tokio::sync::mpsc;

use crate::rathole::cmd::Command;
use crate::rathole::context::CommandSender;
use crate::rathole::frame::Frame;
use crate::rathole::{context, CommandChannel};

pub struct Dispatcher<T> {
    command_read: CommandRead<T>,
    command_write: CommandWrite<T>,

    heartbeat: Heartbeat,
    heartbeat_sender: mpsc::Sender<()>,
    context: context::Context,
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

struct Heartbeat {
    beat_at: Instant,
    receiver: mpsc::Receiver<()>,
}

impl<T: AsyncRead + AsyncWrite + Unpin> Dispatcher<T> {
    pub fn new(stream: T, hash: String) -> (Self, Arc<CommandSender>) {
        let (sender, receiver) = mpsc::channel(1);
        let command_sender = Arc::new(CommandSender::new(hash, sender));
        let context = context::Context::new(command_sender.clone());
        let (read, write) = io::split(stream);
        let command_read = CommandRead::new(read);
        let command_write = CommandWrite::new(write);

        let (heartbeat_sender, heartbeat_receiver) = mpsc::channel(1);
        let heartbeat = Heartbeat {
            beat_at: Instant::now(),
            receiver: heartbeat_receiver,
        };

        (
            Dispatcher {
                command_read,
                command_write,
                receiver,
                context,
                heartbeat,
                heartbeat_sender,
            },
            command_sender,
        )
    }

    pub async fn dispatch(&mut self) -> anyhow::Result<()> {
        tokio::select! {
            r1 = Self::apply_command(
                &mut self.command_read,
                &mut self.heartbeat_sender,
                self.context.clone(),
            ) => r1,
            r2 = Self::recv_command(
                &mut self.command_write,
                &mut self.receiver,
                self.context.clone(),
            ) => r2,
            r3 = self.heartbeat.watch() => r3,
        }
    }

    async fn apply_command(
        command_read: &mut CommandRead<T>,
        heartbead_sender: &mut mpsc::Sender<()>,
        context: context::Context,
    ) -> anyhow::Result<()> {
        loop {
            let mut cn = context.clone();

            let (req_id, cmd) = command_read
                .read_command()
                .await
                .context("Dispatcher can't read command by conn")?;

            heartbead_sender.send(()).await?;

            cn.with_req_id(req_id);
            let op_resp = cmd.apply(cn).await.context("Can't apply command")?;

            if let Some(resp) = op_resp {
                trace!("Send resp {:?}", resp);
                context
                    .command_sender
                    .send_with_id(req_id, resp)
                    .await
                    .context("Can't send command resp")?
            }
        }
    }

    async fn recv_command(
        command_write: &mut CommandWrite<T>,
        receiver: &mut mpsc::Receiver<CommandChannel>,
        context: context::Context,
    ) -> anyhow::Result<()> {
        loop {
            let mut cn = context.clone();
            let oc = receiver.recv().await;
            trace!("recv command {:?}", oc);
            match oc {
                Some((req_id, cmd, rc)) => {
                    match cmd {
                        Command::Resp(_) => {}
                        _ => {
                            cn.with_req_id(req_id).set_req(rc).await;
                        }
                    }
                    command_write
                        .write_command(req_id, cmd)
                        .await
                        .context("Can't write command")?;
                }
                None => return Err(anyhow!("cmd receiver close")),
            };
        }
    }
}

impl<T: AsyncRead> CommandRead<T> {
    pub fn new(read: ReadHalf<T>) -> Self {
        CommandRead {
            read,
            buffer: BytesMut::new(),
        }
    }

    pub async fn read_command(&mut self) -> anyhow::Result<(u64, Command)> {
        let f = self.read_frame().await.context("Can't read frame")?;
        let (req_id, cmd) = Command::from_frame(f).context("Can't cover frame to command")?;
        Ok((req_id, cmd))
    }

    async fn read_frame(&mut self) -> anyhow::Result<Frame> {
        loop {
            if let Some(frame) = self.parse_frame().context("Can't parse frame")? {
                trace!("read frame : {}", frame);
                return Ok(frame);
            }

            if 0 == self
                .read
                .read_buf(&mut self.buffer)
                .await
                .context("Can't read buf for frame")?
            {
                bail!("connection reset by peer");
            }
        }
    }

    fn parse_frame(&mut self) -> anyhow::Result<Option<Frame>> {
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

    pub async fn write_command(&mut self, req_id: u64, cmd: Command) -> anyhow::Result<()> {
        let frame = cmd
            .to_frame(req_id)
            .context("Can't cover command to frame")?;

        trace!("write frame : {}", frame);
        self.write_frame(&frame)
            .await
            .context("Can't write frame to conn")?;
        Ok(())
    }

    async fn write_frame(&mut self, frame: &Frame) -> anyhow::Result<()> {
        match frame {
            Frame::Array(val) => {
                self.write
                    .write_u8(b'*')
                    .await
                    .context("Can't write byte to conn")?;

                self.write_decimal(val.len() as u64).await?;

                for entry in &**val {
                    self.write_value(entry).await?;
                }
            }
            _ => self.write_value(frame).await?,
        }

        self.write.flush().await?;
        Ok(())
    }

    /// Write a frame literal to the stream
    async fn write_value(&mut self, frame: &Frame) -> anyhow::Result<()> {
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
    async fn write_decimal(&mut self, val: u64) -> anyhow::Result<()> {
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

impl<T> Drop for Dispatcher<T> {
    fn drop(&mut self) {
        self.context.notify_shutdown.send(()).ok();
        info!("Drop Dispatcher")
    }
}

impl Heartbeat {
    pub async fn watch(&mut self) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = self.receiver.recv() =>{
                    self.beat_at = Instant::now();
                },
                _ = interval.tick() => {
                    let elapsed = self.beat_at.elapsed().as_secs();
                    debug!("last heartbeat at {}s ago", elapsed);
                    if elapsed > 60 {
                        return Err(anyhow!("no have heartbeat in 30s"));
                    }
                }
            }
        }
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
            Frame::Bulk(Bytes::from(vec![0x65, 0x66])),
        ]);
        let f = frame_write_read(&frame).await;
        assert_eq!(format!("{:?}", frame), format!("{:?}", f))
    }
}
