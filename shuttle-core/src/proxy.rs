use anyhow::Context;
use socks5_proto::{
    handshake::{self, Method},
    Address, Command, Reply, Request, Response,
};
use tokio::{
    io::{copy_bidirectional, AsyncRead, AsyncWrite},
    net::TcpStream,
};

use crate::dial::Dial;

#[derive(Debug)]
pub struct ProxyConnection<T> {
    ts: TcpStream,
    dial: Box<dyn Dial<T>>,
}

impl<T: AsyncRead + AsyncWrite + Unpin> ProxyConnection<T> {
    pub fn new(ts: TcpStream, dial: Box<dyn Dial<T>>) -> Self {
        Self { ts, dial }
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
        debug!(
            "Socks proxy connection {:?} to {:?}",
            self.ts.peer_addr().ok(),
            self.ts.local_addr().ok()
        );

        let _req = handshake::Request::read_from(&mut self.ts)
            .await
            .context("Socks handshake failed")?;

        // no neet auth
        let resp = handshake::Response::new(Method::NONE);

        resp.write_to(&mut self.ts)
            .await
            .context("Socks write response failed")?;

        let req = Request::read_from(&mut self.ts)
            .await
            .context("Socks read request failed")?;

        let addr = req.address;

        match req.command {
            Command::Connect => {
                let target = self.dial.dial(addr).await;
                if let Ok(mut target) = target {
                    self.socks_reply(Reply::Succeeded, Address::unspecified())
                        .await?;

                    copy_bidirectional(&mut target, &mut self.ts).await.ok();
                } else {
                    self.socks_reply(Reply::HostUnreachable, Address::unspecified())
                        .await?;
                }
            }
            _ => {
                self.socks_reply(Reply::CommandNotSupported, Address::unspecified())
                    .await?;
            }
        }
        Ok(())
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

    async fn socks_reply(&mut self, reply: Reply, addr: Address) -> anyhow::Result<()> {
        let resp = Response::new(reply, addr);
        resp.write_to(&mut self.ts)
            .await
            .context("Scoks write reply response failed")
    }
}

#[cfg(test)]
mod tests {}
