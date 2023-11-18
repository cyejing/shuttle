use anyhow::Context;
use socks5_proto::{
    handshake::{self, Method},
    Address, Command, Reply, Request as SocksRequest, Response,
};
use tokio::{
    io::{copy_bidirectional, AsyncRead, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};

use crate::{dial::Dial, proto::http_connect};

#[derive(Debug)]
pub struct ProxyConnection<T> {
    ts: TcpStream,
    dial: Box<dyn Dial<T>>,
}

impl<T: AsyncRead + AsyncWrite + Unpin> ProxyConnection<T> {
    pub fn new(ts: TcpStream, dial: Box<dyn Dial<T>>) -> Self {
        Self { ts, dial }
    }

    pub async fn handle(self) {
        let mut first_bit = [0u8];
        if let Err(e) = self.ts.peek(&mut first_bit).await {
            error!("Can't peek first_bit err: {e}");
            return;
        }

        let ret = if first_bit[0] == socks5_proto::SOCKS_VERSION {
            self.handle_socks().await
        } else {
            self.handle_http().await
        };
        if let Err(e) = ret {
            error!("ProxyServer handle stream err: {e:?}");
        };
    }

    async fn handle_socks(mut self) -> anyhow::Result<()> {
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

        let req = SocksRequest::read_from(&mut self.ts)
            .await
            .context("Socks read request failed")?;

        let addr = req.address;

        info!("Requested socks connection to: {addr}");

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

    async fn handle_http(mut self) -> anyhow::Result<()> {
        debug!(
            "Http proxy connection {:?} to {:?}",
            self.ts.peer_addr().ok(),
            self.ts.local_addr().ok()
        );
        let buf = http_connect::read_http_request_end(&mut self.ts)
            .await
            .context("Http proxy read http request end failed")?;
        info!(
            "Http proxy read buf: \n{}",
            String::from_utf8_lossy(buf.as_slice())
        );
        match http_connect::HttpConnectRequest::parse(buf.as_slice()) {
            Ok(req) => {
                let mut target = TcpStream::connect(&req.host)
                    .await
                    .context(format!("Http proxy connect addr {} failed", req.host))?;
                info!("Http proxy connect {}", req.host);

                if let Some(data) = req.nugget {
                    target
                        .write_all(&data.data())
                        .await
                        .context("Http proxy target write_all buf failed")?;
                } else {
                    self.ts
                        .write("HTTP/1.1 200 OK\r\n\r\n".as_bytes())
                        .await
                        .context("Http proxy write response failed")?;
                }

                if let Ok((a, b)) = copy_bidirectional(&mut self.ts, &mut target).await {
                    info!("Http proxy copy {} end {}={}", req.host, a, b);
                }
            }
            Err(_e) => {
                error!("Http proxy BAD_REQUEST");
                self.ts
                    .write("HTTP/1.1 400 BAD_REQUEST\r\n\r\n".as_bytes())
                    .await
                    .context("Http proxy write response failed")?;
            }
        }
        Ok(())
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
