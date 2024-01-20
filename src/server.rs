use anyhow::{anyhow, Context};
use shuttle_station::dial::{Dial, DirectDial};
use shuttle_station::peekable::{AsyncPeek, PeekableStream};
use shuttle_station::proto::{self, trojan};
use tracing::{info_span, Instrument};

use crate::config::Addr;
use crate::gen_traceid;
use crate::rathole::dispatcher::Dispatcher;
use crate::store::ServerStore;
use shuttle_station::tls::make_tls_acceptor;
use tokio::io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

pub async fn start_server(addr: &Addr, store: ServerStore) {
    let addr_str = &addr.addr;
    let listener = TcpListener::bind(addr_str)
        .await
        .context(format!("Can't bind server port {}", addr_str))
        .unwrap();
    info!("Server listener addr : {}", addr_str);
    let acceptor = if addr.ssl_enable {
        make_tls_acceptor(addr.cert_loaded.clone(), addr.key_loaded.as_ref())
    } else {
        None
    };

    tokio::spawn(async move {
        while let Ok((ts, _)) = listener.accept().await {
            let store = store.clone();
            let acceptor = acceptor.clone();
            let span = info_span!("trace", id = gen_traceid());
            tokio::spawn(async move { server_handle(ts, store, acceptor).instrument(span).await });
        }
    });
}

async fn server_handle(ts: TcpStream, store: ServerStore, acceptor: Option<TlsAcceptor>) {
    match acceptor {
        Some(tls_acc) => {
            match tls_acc.accept(ts).await {
                Ok(tls_ts) => {
                    ServerHandler::new(PeekableStream::new(tls_ts), store)
                        .handle()
                        .await
                }
                Err(e) => {
                    error!("Accept tls connection err : {:?}", e);
                }
            };
        }
        None => {
            ServerHandler::new(PeekableStream::new(ts), store)
                .handle()
                .await
        }
    }
}

struct ServerHandler<T> {
    inner: T,
    hash: Option<String>,
    store: ServerStore,
}

impl<T> ServerHandler<T>
where
    T: AsyncRead + AsyncWrite + AsyncPeek + Unpin,
{
    pub fn new(inner: T, store: ServerStore) -> Self {
        Self {
            inner,
            hash: None,
            store,
        }
    }

    async fn handle(&mut self) {
        let ret = match self.detect_head().await {
            Ok(ConnType::Trojan) => self.handle_trojan().await,
            Ok(ConnType::Rathole) => self.handle_rathole().await,
            Ok(ConnType::Proxy) => self.handle_proxy().await,
            Err(e) => Err(e).context("Can't detect head"),
        };
        if let Err(e) = ret {
            error!("Server handle err: {e:?}");
        };
    }

    async fn detect_head(&mut self) -> anyhow::Result<ConnType> {
        let head = proto::trojan::Request::peek_head(&mut self.inner)
            .await
            .context("Server peek head failed")?;
        if head.len() < 56 {
            return Ok(ConnType::Proxy);
        }

        let hash_str = String::from_utf8(head).context("Trojan hash to string failed")?;

        let trojan = self.store.get_trojan();
        let rathole = self.store.get_rahole();

        if trojan.password_hash.contains_key(&hash_str) {
            debug!("detect trojan {}", hash_str);
            self.hash = Some(hash_str);
            Ok(ConnType::Trojan)
        } else if rathole.password_hash.contains_key(&hash_str) {
            debug!("detect rathole {}", hash_str);
            self.hash = Some(hash_str);
            Ok(ConnType::Rathole)
        } else {
            debug!("detect proxy");
            Ok(ConnType::Proxy)
        }
    }

    async fn handle_trojan(&mut self) -> anyhow::Result<()> {
        let stream = &mut self.inner;
        let req = trojan::Request::read_from(stream)
            .await
            .context("Trojan request read failed")?;
        let addr = req.address.clone();

        debug!("Trojan start connect {addr}");

        let mut remote_ts = DirectDial::default()
            .dial(addr.clone())
            .await
            .context(format!("Trojan connect remote addr {} failed", req.address))?;

        info!("Trojan Requested to {}", addr);
        if let Ok((a, b)) = copy_bidirectional(stream, &mut remote_ts).await {
            info!(
                "Trojan copy end for {} traffic: {}<=>{} total: {}",
                req.address,
                a,
                b,
                a + b
            );
        }

        Ok(())
    }

    async fn handle_proxy(&mut self) -> anyhow::Result<()> {
        let stream = &mut self.inner;
        let trojan = self.store.get_trojan();

        match trojan.local_addr {
            Some(ref local_addr) => {
                info!("requested proxy local {}", local_addr);
                let mut local_ts = TcpStream::connect(local_addr)
                    .await
                    .context(format!("Proxy can't connect addr {}", local_addr))?;
                debug!("Proxy connect success {:?}", &trojan.local_addr);

                copy_bidirectional(stream, &mut local_ts).await.ok();
            }
            None => {
                info!("response not found");
                resp_html(stream).await
            }
        }

        Ok(())
    }

    async fn handle_rathole(&mut self) -> anyhow::Result<()> {
        let stream = &mut self.inner;
        let _ = stream.drain();
        let _crlf = stream.read_u16().await?;
        let hash_str = self
            .hash
            .clone()
            .ok_or_else(|| anyhow!("rathole hash empty!"))?;
        let (mut dispatcher, cs) = Dispatcher::new(stream, hash_str.clone());
        self.store.set_cmd_sender(cs).await;

        dispatcher.dispatch().await.ok();

        self.store.remove_cmd_sender(&hash_str).await;
        Ok(())
    }
}

pub enum ConnType {
    Trojan,
    Rathole,
    Proxy,
}

async fn resp_html<T: AsyncRead + AsyncWrite + Unpin>(stream: &mut T) {
    stream
        .write_all(
            &b"HTTP/1.0 404 Not Found\r\n\
                Content-Type: text/plain; charset=utf-8\r\n\
                Content-length: 13\r\n\r\n\
                404 Not Found"[..],
        )
        .await
        .unwrap();
}
