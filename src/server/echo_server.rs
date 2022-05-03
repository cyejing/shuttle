use std::net::SocketAddr;

use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::{TcpListener, TcpStream};

pub struct EchoServer {
    pub addr: String,
}

pub struct Conn {
    sd: SocketAddr,
    stream: BufWriter<TcpStream>,
    buffer: BytesMut,
}

impl EchoServer {
    #[allow(unused)]
    pub fn new(addr: String) -> EchoServer {
        EchoServer {
            addr
        }
    }

    #[allow(unused)]
    pub async fn start(self) {
        let ln = TcpListener::bind(self.addr).await.unwrap();
        loop {
            let (ts, sd) = ln.accept().await.unwrap();

            let mut conn = Conn::new(ts, sd);
            tokio::spawn(async move {
                conn.reading().await;
            });
        }
    }
}

impl Conn {
    pub fn new(ts: TcpStream, sd: SocketAddr) -> Conn {
        Conn {
            sd,
            stream: BufWriter::new(ts),
            buffer: BytesMut::with_capacity(4 * 1024),
        }
    }

    pub async fn reading(&mut self) {
        loop {
            let i = self.stream.read_buf(&mut self.buffer).await.unwrap();
            if 0 < i {
                println!("i:{}", i);
                let vec = self.buffer.to_vec();
                self.stream.write_all(&vec).await.unwrap();
                self.stream.flush().await.unwrap();
            } else {
                println!("read EOF {}", self.sd);
                break;
            };
        }
    }
}


