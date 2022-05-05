use std::sync::Arc;

use log::{debug, error};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use crate::config::Addr;
use crate::tls::make_tls_acceptor;

pub struct TlsServer {
    pub addr: Addr,
}

impl TlsServer {
    pub fn new(addr: Addr) -> TlsServer {
        TlsServer {
            addr,
        }
    }

    pub async fn start(self) -> crate::Result<()> {
        let addr = &self.addr.addr;
        let cert_loaded = self.addr.cert_loaded;
        let mut key_loaded = self.addr.key_loaded;

        let acceptor = Arc::new(make_tls_acceptor(cert_loaded, key_loaded.remove(0)));

        let listener = TcpListener::bind(addr).await?;

        loop {
            let acc = listener.accept().await;
            match acc {
                Ok((ts, sd)) => {
                    let acceptor = acceptor.clone();
                    tokio::spawn(async move {
                        let acc = acceptor.accept(ts).await;
                        match acc {
                            Ok(mut tls_ts) => {
                                debug!("accept new connect!");
                                // let mut stream = ts;
                                tls_ts.write_all(
                                    &b"HTTP/1.0 200 ok\r\n\
                                    Connection: keep-alive\r\n\
                                    Content-Type: text/plain; charset=utf-8\r\n\
                                    Content-length: 12\r\n\
                                    \r\n\
                                    Hello world!"[..],
                                ).await.unwrap();
                                println!("Hello: {}", sd);
                            }
                            Err(e) => {
                                error!("Accept connect err : {}",e)
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("accept connection err : {}",e)
                }
            }
        }
    }
}
