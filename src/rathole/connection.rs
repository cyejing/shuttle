use std::collections::HashMap;
use std::io::Cursor;

use bytes::{Buf, BytesMut};
use log::{info};
use tokio::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;

use crate::rathole::cmd::Command;
use crate::rathole::frame::Frame;

pub struct ConnectionHolder<T: AsyncRead + AsyncWrite + Unpin> {
    req_map: HashMap<String, Req>,
    conn: Connection<T>,
    cmd_receiver: mpsc::Receiver<Command>,
}

impl<T: AsyncRead + AsyncWrite + Unpin> ConnectionHolder<T> {
    pub fn new(conn: Connection<T>, cmd_receiver: mpsc::Receiver<Command>) -> Self {
        ConnectionHolder {
            req_map: HashMap::new(),
            conn,
            cmd_receiver,
        }
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                rof = self.conn.read_frame() =>{
                    info!("read frame {:?}", rof.unwrap());
                },
                oc = self.cmd_receiver.recv() => {
                    match oc {
                        Some(cmd)=>info!("read cmd :{:?}", cmd),
                        None=>panic!("receiver end"),
                    }
                },
            }
        }
    }
}


#[derive(Debug)]
pub struct CmdSender {
    pub rh_name: String,
    pub sender: mpsc::Sender<Command>,
}

#[derive(Debug)]
struct Req {}

pub struct Connection<T: AsyncRead + AsyncWrite + Unpin> {
    stream: BufWriter<T>,
    buffer: BytesMut,
}

impl<T: AsyncRead + AsyncWrite + Unpin> Connection<T> {
    pub fn new(socket: T) -> Self {
        Connection {
            stream: BufWriter::new(socket),
            buffer: BytesMut::with_capacity(4 * 1024),
        }
    }

    pub async fn read_frame(&mut self) -> crate::Result<Option<Frame>> {
        loop {
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            if 0 == self.stream.read_buf(&mut self.buffer).await? {
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

    pub async fn write_frame(&mut self, frame: &Frame) -> io::Result<()> {
        match frame {
            Frame::Array(val) => {
                self.stream.write_u8(b'*').await?;

                self.write_decimal(val.len() as u64).await?;

                for entry in &**val {
                    self.write_value(entry).await?;
                }
            }
            _ => self.write_value(frame).await?,
        }

        self.stream.flush().await
    }


    /// Write a frame literal to the stream
    async fn write_value(&mut self, frame: &Frame) -> io::Result<()> {
        match frame {
            Frame::Simple(val) => {
                self.stream.write_u8(b'+').await?;
                self.stream.write_all(val.as_bytes()).await?;
                self.stream.write_all(b"\r\n").await?;
            }
            Frame::Error(val) => {
                self.stream.write_u8(b'-').await?;
                self.stream.write_all(val.as_bytes()).await?;
                self.stream.write_all(b"\r\n").await?;
            }
            Frame::Integer(val) => {
                self.stream.write_u8(b':').await?;
                self.write_decimal(*val).await?;
            }
            Frame::Null => {
                self.stream.write_all(b"$-1\r\n").await?;
            }
            Frame::Bulk(val) => {
                let len = val.len();

                self.stream.write_u8(b'$').await?;
                self.write_decimal(len as u64).await?;
                self.stream.write_all(val).await?;
                self.stream.write_all(b"\r\n").await?;
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
        self.stream.write_all(&buf.get_ref()[..pos]).await?;
        self.stream.write_all(b"\r\n").await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Cursor;

    use bytes::Bytes;

    use crate::common::consts;
    use crate::rathole::connection::Connection;
    use crate::rathole::frame::Frame;

    #[tokio::test]
    async fn test_read_write_frame_simple() {
        let frame = Frame::Simple("hello".to_string());

        let mut cell = RefCell::new(Vec::new());
        let mut connection = Connection::new(Cursor::new(cell.get_mut()));
        connection.write_frame(&frame).await.unwrap();
        let read = cell.get_mut().as_slice().clone();
        println!("{:?}", read);

        let mut read_conn = Connection::new(Cursor::new(Vec::from(read)));
        let f = read_conn.read_frame().await.unwrap().unwrap();
        println!("{:?}", f);
        assert!(frame.eq(&"hello"))
    }

    #[tokio::test]
    async fn test_read_write_frame_array() {
        let frame = Frame::Array(vec![Frame::Simple("hello".to_string()),
                                      Frame::Integer(2),
                                      Frame::Bulk(Bytes::from(vec![0x01, 0x02]))]);

        let mut cell = RefCell::new(Vec::new());
        let mut connection = Connection::new(Cursor::new(cell.get_mut()));
        connection.write_frame(&frame).await.unwrap();
        let read = cell.get_mut().as_slice().clone();
        println!("{:?}", read);

        let mut read_conn = Connection::new(Cursor::new(Vec::from(read)));
        let f = read_conn.read_frame().await.unwrap().unwrap();
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
