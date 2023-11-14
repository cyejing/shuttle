use anyhow::{anyhow, Context};
use socks5_proto::handshake;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};

#[derive(Debug)]
struct ProxyConnection {
    ts: TcpStream,
}

impl ProxyConnection {
    pub fn new(ts: TcpStream) -> Self {
        Self { ts }
    }
    pub async fn handle(&mut self) -> anyhow::Result<()> {
        let mut first_bit = [0u8];
        self.ts
            .peek(&mut first_bit)
            .await
            .context("Can't peek first_bit")?;

        if first_bit[0] == socks5_proto::SOCKS_VERSION {
            self.handle_socks().await
        } else {
            self.handle_http().await
        }
    }

    async fn handle_socks(&mut self) -> anyhow::Result<()> {
        let mut ts = &mut self.ts;
        debug!(
            "Socks proxy connection {:?} to {:?}",
            ts.peer_addr().ok(),
            ts.local_addr().ok()
        );

        let req = match handshake::Request::read_from(&mut ts).await {
            Ok(req) => req,
            Err(err) => {
                let _ = ts.shutdown().await;
                return Err(err).context("Socks handshake failed");
            }
        };
        // no neet auth

        todo!()
    }

    async fn handle_http(&mut self) -> anyhow::Result<()> {
        let ts = &self.ts;
        debug!(
            "Http proxy connection {:?} to {:?}",
            ts.peer_addr().ok(),
            ts.local_addr().ok()
        );

        todo!()
    }
}

#[derive(Debug)]
pub struct ProxyServer {
    addr: String,
}

impl ProxyServer {
    pub async fn start(&self) -> anyhow::Result<()> {
        let addr = &self.addr;
        let listener = TcpListener::bind(addr)
            .await
            .context(format!("Can't Listen socks addr {}", addr))?;

        tokio::spawn(async move {
            while let Ok((ts, _)) = listener.accept().await {
                tokio::spawn(async move {
                    if let Err(e) = ProxyConnection::new(ts).handle().await {
                        error!("ProxyServer handle stream err: {e}");
                    }
                });
            }
        });

        Ok(())
    }
}
