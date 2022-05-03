use std::io;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_rustls::rustls;
use tokio_rustls::TlsAcceptor;
use crate::config::Addr;

pub struct TlsServer {
    pub addr: Addr
}

impl TlsServer {
    pub fn new(addr: Addr) -> TlsServer {
        TlsServer {
            addr,
        }
    }

    pub async fn start(self) {
        let addr = &self.addr.addr;
        let certs = self.addr.cert_vec;
        let mut keys = self.addr.key_vec;

        let config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, keys.remove(0))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));

        let listener = TcpListener::bind(addr).await.unwrap();

        loop {
            let (ts, sd) = listener.accept().await.expect("accept conn err");
            let acceptor = acceptor.clone();

            let handle = tokio::spawn(async move {
                let mut stream = acceptor.accept(ts).await?;
                println!("read tls");
                stream.write_all(
                    &b"HTTP/1.0 200 ok\r\n\
                    Connection: keep-alive\r\n\
                    Content-Type: text/plain; charset=utf-8\r\n\
                    Content-length: 12\r\n\
                    \r\n\
                    Hello world!"[..],
                ).await?;
                println!("Hello: {}", sd);
                Ok(()) as io::Result<()>
            });

            let result = handle.await.unwrap();
            println!("{:?}",result)

        }
    }
}
