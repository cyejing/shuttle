use std::fmt;

use anyhow::Context;
use async_trait::async_trait;
use futures::SinkExt;
use socks5_proto::Address;
use socks5_proto::Command;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::MaybeTlsStream;

use crate::proto::trojan;
use crate::tls::make_server_name;
use crate::tls::make_tls_connector;
use crate::websocket::WebSocketCopyStream;

#[async_trait]
pub trait Dial<T>: Sync + Send + fmt::Debug {
    async fn dial(&self, addr: Address) -> anyhow::Result<T>;
}

#[derive(Debug, Default)]
pub struct DirectDial {}
#[derive(Debug)]
#[allow(dead_code)]
pub struct TrojanDial {
    remote_addr: String,
    hash: String,
    ssl_enable: bool,
    invalid_certs: bool,
}
#[derive(Debug)]
#[allow(dead_code)]
pub struct WebSocketDial {
    remote_addr: String,
    hash: String,
}

impl TrojanDial {
    pub fn new(remote_addr: String, hash: String, ssl_enable: bool, invalid_certs: bool) -> Self {
        Self {
            remote_addr,
            hash,
            ssl_enable,
            invalid_certs,
        }
    }
}

impl WebSocketDial {
    pub fn new(remote_addr: String, hash: String) -> Self {
        Self { remote_addr, hash }
    }
}

#[async_trait]
impl Dial<TcpStream> for DirectDial {
    async fn dial(&self, addr: Address) -> anyhow::Result<TcpStream> {
        let target = match addr {
            Address::DomainAddress(domain, port) => {
                let domain = String::from_utf8_lossy(&domain);
                TcpStream::connect((domain.as_ref(), port))
                    .await
                    .context(format!("DirectDial connect {domain} failed"))?
            }
            Address::SocketAddress(addr) => TcpStream::connect(addr)
                .await
                .context(format!("DirectDial connect {addr} failed"))?,
        };
        Ok(target)
    }
}

#[async_trait]
impl Dial<TcpStream> for TrojanDial {
    async fn dial(&self, addr: Address) -> anyhow::Result<TcpStream> {
        let remote_addr = &self.remote_addr;
        let mut remote_ts = TcpStream::connect(remote_addr)
            .await
            .context(format!("Trojan can't connect remote {}", remote_addr))?;

        let req = trojan::Request::new(self.hash.clone(), Command::Connect, addr);
        req.write_to(&mut remote_ts).await?;

        Ok(remote_ts)
    }
}

#[async_trait]
impl Dial<TlsStream<TcpStream>> for TrojanDial {
    async fn dial(&self, addr: Address) -> anyhow::Result<TlsStream<TcpStream>> {
        let remote_addr = &self.remote_addr;
        let remote_ts = TcpStream::connect(remote_addr)
            .await
            .context(format!("Trojan can't connect remote {}", remote_addr))?;

        let server_name = make_server_name(remote_addr.as_str())?;
        let mut remote_ts_ssl = make_tls_connector(self.invalid_certs)
            .connect(server_name, remote_ts)
            .await
            .context("Trojan can't connect tls")?;

        let req = trojan::Request::new(self.hash.clone(), Command::Connect, addr);
        req.write_to(&mut remote_ts_ssl).await?;

        Ok(remote_ts_ssl)
    }
}

#[async_trait]
impl Dial<WebSocketCopyStream<MaybeTlsStream<TcpStream>>> for WebSocketDial {
    async fn dial(
        &self,
        addr: Address,
    ) -> anyhow::Result<WebSocketCopyStream<MaybeTlsStream<TcpStream>>> {
        let remote_addr = &self.remote_addr;
        let (mut ws, _) = connect_async(remote_addr)
            .await
            .context(format!("WebSocket can't connect remote {}", remote_addr))?;

        let mut buf: Vec<u8> = vec![];
        let req = trojan::Request::new(self.hash.clone(), Command::Connect, addr);
        req.write_to_buf(&mut buf);

        ws.send(Message::Binary(buf))
            .await
            .context("WebSocket can't send")?;

        Ok(WebSocketCopyStream::new(ws))
    }
}
