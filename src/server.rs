use std::io::Cursor;

use log::{debug, error, info};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener};

use crate::config::{Addr, ServerStore};
use crate::tls::make_tls_acceptor;

pub struct TcpServer {
    pub addr: Addr,
    pub ss: ServerStore,
}

impl TcpServer {
    pub fn new(addr: Addr, ss: ServerStore) -> Self {
        TcpServer {
            addr,
            ss,
        }
    }

    pub async fn start(self) {
        let addr = &self.addr.addr;

        let store = self.ss;

        let lis = TcpListener::bind(addr).await.expect("bind port failed");
        info!("Tcp Server listener addr : {}", addr);
        loop {
            match lis.accept().await {
                Ok((ts, _sd)) => {
                    let ss_store = store.clone();
                    tokio::spawn(async move {
                        let mut stream = ServerStream::new(ts, ss_store);
                        if let Err(e) = stream.handle().await{
                            error!("tls handle server stream err : {}", e)
                        }
                    });
                }
                Err(e) => {
                    error!("accept connection err,{}", e)
                }
            }
        }
    }
}

pub struct TlsServer {
    pub addr: Addr,
    pub ss: ServerStore,
}

impl TlsServer {
    pub fn new(addr: Addr, ss: ServerStore) -> Self {
        TlsServer {
            addr,
            ss,
        }
    }

    pub async fn start(self) {
        let addr = &self.addr.addr;
        let cert_loaded = self.addr.cert_loaded;
        let mut key_loaded = self.addr.key_loaded;

        let store = self.ss;

        let acceptor = make_tls_acceptor(cert_loaded, key_loaded.remove(0));

        let lis = TcpListener::bind(addr).await.expect("bind port failed");
        info!("Tls Server listener addr : {}", addr);
        loop {
            match lis.accept().await {
                Ok((ts, _sd)) => {
                    let tls_acc = acceptor.clone();
                    match tls_acc.accept(ts).await {
                        Ok(tls_ts) => {
                            let ss_store = store.clone();
                            tokio::spawn(async move {
                                let mut stream = ServerStream::new(tls_ts, ss_store);
                                if let Err(e) = stream.handle().await {
                                    error!("tls handle server stream err : {}", e)
                                }
                            });
                        }
                        Err(e) => {
                            error!("accept tls connection err,{}", e)
                        }
                    }
                }
                Err(e) => {
                    error!("accept connection err,{}", e)
                }
            }
        }
    }
}


pub struct ServerStream<T: AsyncRead + AsyncWrite + Unpin> {
    ts: T,
    store: ServerStore,
}

impl<T: AsyncRead + AsyncWrite + AsyncWriteExt + Unpin> ServerStream<T> {
    pub async fn handle(&mut self) -> crate::Result<()> {
        debug!("accept new connect! {:?}", self.store);
        self.peek_trojan().await?;
        self.ts.write_all(
            &b"HTTP/1.0 200 ok\r\n\
                    Connection: keep-alive\r\n\
                    Content-Type: text/plain; charset=utf-8\r\n\
                    Content-length: 12\r\n\
                    \r\n\
                    Hello world!"[..],
        ).await.unwrap();
        Ok(())
    }

    pub async fn peek_trojan(&mut self) -> crate::Result<()> {
        // let bytes = bytes::Bytes::new();
        // let mut buff = BufReader::new(&mut self.ts);
        // let i = buff.read_u8().await.unwrap();
        //
        // println!("{}",i);

        Ok(())
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> ServerStream<T> {
    pub fn new(ss: T, store: ServerStore) -> Self {
        ServerStream {
            ts: ss,
            store,
        }
    }
}

