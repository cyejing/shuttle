use std::fmt;

use anyhow::Context;
use async_trait::async_trait;
use socks5_proto::Address;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::MaybeTlsStream;

use crate::{
    tls::{make_server_name, make_tls_connector},
    websocket::WebSocketCopyStream,
};

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
                    .context("DirectDial connect {domain} failed")?
            }
            Address::SocketAddress(addr) => TcpStream::connect(addr)
                .await
                .context("DirectDial connect {addr} failed")?,
        };
        Ok(target)
    }
}

#[async_trait]
impl Dial<TlsStream<TcpStream>> for DirectDial {
    async fn dial(&self, addr: Address) -> anyhow::Result<TlsStream<TcpStream>> {
        let target = match addr {
            Address::DomainAddress(domain, port) => {
                let domain = String::from_utf8_lossy(&domain);
                TcpStream::connect((domain.as_ref(), port))
                    .await
                    .context("DirectDial connect {domain} failed")?
            }
            Address::SocketAddress(addr) => TcpStream::connect(addr)
                .await
                .context("DirectDial connect {addr} failed")?,
        };

        let server_name = make_server_name("")?;
        let ssl_target = make_tls_connector(true)
            .connect(server_name, target)
            .await
            .context("Trojan can't connect tls")?;
        Ok(ssl_target)
    }
}

#[async_trait]
impl Dial<TcpStream> for TrojanDial {
    async fn dial(&self, _addr: Address) -> anyhow::Result<TcpStream> {
        todo!()
    }
}

#[async_trait]
impl Dial<TlsStream<TcpStream>> for TrojanDial {
    async fn dial(&self, _addr: Address) -> anyhow::Result<TlsStream<TcpStream>> {
        todo!()
    }
}

#[async_trait]
impl Dial<WebSocketCopyStream<MaybeTlsStream<TcpStream>>> for WebSocketDial {
    async fn dial(
        &self,
        _addr: Address,
    ) -> anyhow::Result<WebSocketCopyStream<MaybeTlsStream<TcpStream>>> {
        todo!()
    }
}
