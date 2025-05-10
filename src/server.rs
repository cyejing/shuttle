use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use borer_core::proto::{self};
use borer_core::stats_server;
use borer_core::stream::acceptor::{Acceptor, MaybeTlsStream};
use borer_core::stream::peekable::{AsyncPeek, PeekableStream};
use borer_core::trojan::TrojanConnection;
use tracing::{Instrument, info_span};

use crate::config::Addr;
use crate::rathole::dispatcher::Dispatcher;
use crate::setup::gen_traceid;
use crate::store::ServerStore;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
struct State {
    acceptor: Arc<Acceptor>,
}

impl State {
    fn new(addr: &Addr) -> State {
        let acceptor = Acceptor::new(addr.cert.clone(), addr.key.clone());

        State {
            acceptor: Arc::new(acceptor),
        }
    }
}

pub async fn start_server(addr: &Addr, store: ServerStore) {
    let addr_str = &addr.addr;
    let listener = TcpListener::bind(addr_str)
        .await
        .context(format!("Can't bind server port {}", addr_str))
        .unwrap();
    info!("server up and running. listen: {}", addr_str);
    let state = State::new(addr);

    tokio::spawn(async move {
        while let Ok((ts, _)) = listener.accept().await {
            let store = store.clone();
            let state = state.clone();
            let span = info_span!("trace", id = gen_traceid());
            tokio::spawn(async move {
                if let Err(e) = server_handle(ts, store, state).instrument(span).await {
                    error!("Server handle failed. e:{e:?}");
                }
            });
        }
    });
}

pub fn start_stats_server(addr: Option<String>, secret: Option<String>) {
    if let (Some(addr), Some(secret)) = (addr, secret) {
        stats_server::StatsServer::new(&addr, &secret).serve();
    }
}

async fn server_handle(ts: TcpStream, store: ServerStore, state: State) -> anyhow::Result<()> {
    let peer_addr = ts.peer_addr()?;
    let ts = state.acceptor.accept(ts).await?;
    let mut peek_ts = PeekableStream::new(ts);

    match detect_head(&mut peek_ts, &store).await? {
        ConnType::Trojan(user) => handle_trojan(&mut peek_ts, user, peer_addr).await,
        ConnType::Rathole(hash) => handle_rathole(&mut peek_ts, &store, hash).await,
        ConnType::Proxy => handle_proxy(&mut peek_ts, &store).await,
    }
}

async fn detect_head(
    ts: &mut PeekableStream<MaybeTlsStream<TcpStream>>,
    store: &ServerStore,
) -> anyhow::Result<ConnType> {
    let head = proto::trojan::Request::peek_head(ts)
        .await
        .context("Server peek head failed")?;
    if head.len() < 56 {
        return Ok(ConnType::Proxy);
    }

    let hash_str = String::from_utf8(head).context("Trojan hash to string failed")?;

    let trojan = store.get_trojan();
    let rathole = store.get_rahole();

    if trojan.password_hash.contains_key(&hash_str) {
        debug!("detect trojan {}", hash_str);
        Ok(ConnType::Trojan(hash_str))
    } else if rathole.password_hash.contains_key(&hash_str) {
        debug!("detect rathole {}", hash_str);
        Ok(ConnType::Rathole(hash_str))
    } else {
        debug!("detect proxy");
        Ok(ConnType::Proxy)
    }
}

async fn handle_trojan(
    ts: &mut PeekableStream<MaybeTlsStream<TcpStream>>,
    user: String,
    peer_addr: SocketAddr,
) -> anyhow::Result<()> {
    TrojanConnection::new(ts, user, peer_addr).handle().await
}

async fn handle_proxy(
    ts: &mut PeekableStream<MaybeTlsStream<TcpStream>>,
    store: &ServerStore,
) -> anyhow::Result<()> {
    let trojan = store.get_trojan();

    match trojan.local_addr {
        Some(ref local_addr) => {
            info!("requested proxy local {}", local_addr);
            let mut local_ts = TcpStream::connect(local_addr)
                .await
                .context(format!("Proxy can't connect addr {}", local_addr))?;
            debug!("Proxy connect success {:?}", &trojan.local_addr);

            copy_bidirectional(ts, &mut local_ts).await.ok();
        }
        None => {
            info!("response not found");
            resp_html(ts).await
        }
    }

    Ok(())
}

async fn handle_rathole(
    ts: &mut PeekableStream<MaybeTlsStream<TcpStream>>,
    store: &ServerStore,
    hash: String,
) -> anyhow::Result<()> {
    let _ = ts.drain();
    let _crlf = ts.read_u16().await?;
    let (mut dispatcher, cs) = Dispatcher::new(ts, hash.clone());
    store.set_cmd_sender(cs).await;

    dispatcher.dispatch().await.ok();

    store.remove_cmd_sender(&hash).await;
    Ok(())
}

pub enum ConnType {
    Trojan(String),
    Rathole(String),
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
