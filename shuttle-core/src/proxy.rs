use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use hyper::{header, server::conn, service::service_fn, Body};
use socks5_proto::{
    handshake::{self, Method},
    Address, Command, Reply, Request as SocksRequest, Response,
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

    pub async fn handle(&mut self) {
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

    async fn handle_http(&mut self) -> anyhow::Result<()> {
        debug!(
            "Http proxy connection {:?} to {:?}",
            self.ts.peer_addr().ok(),
            self.ts.local_addr().ok()
        );
        let host: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let host_move = host.clone();

        if let Err(http_err) = conn::Http::new()
            .http1_only(true)
            .http1_keep_alive(true)
            .serve_connection(
                &mut self.ts,
                service_fn(move |req| http_connect(req, host_move.clone())),
            )
            .await
        {
            error!("Error while serving HTTP connection: {}", http_err);
        }

        // todo
        // if let Some(host) = host.lock().unwrap().take() {
        let mut target = TcpStream::connect("www.baidu.com:443").await.unwrap();
        copy_bidirectional(&mut target, &mut self.ts).await.ok();
        // }

        Ok(())
    }

    async fn socks_reply(&mut self, reply: Reply, addr: Address) -> anyhow::Result<()> {
        let resp = Response::new(reply, addr);
        resp.write_to(&mut self.ts)
            .await
            .context("Scoks write reply response failed")
    }
}

async fn http_connect(
    req: hyper::Request<Body>,
    host_call: Arc<Mutex<Option<String>>>,
) -> anyhow::Result<hyper::Response<String>> {
    if let Some(host) = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
    {
        info!("{},{},{}", req.method(), req.uri(), host);
        let _ = host_call.lock().unwrap().insert(host.to_string());
    }

    Ok(hyper::Response::new("hi".to_string()))
}

#[cfg(test)]
mod tests {}
