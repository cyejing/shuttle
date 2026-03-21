use std::sync::Arc;

use anyhow::Context;
use borer_core::masquerade::MasqueradeConnection;
use borer_core::proto::{self};
use borer_core::stats_server;
use borer_core::stream::acceptor::{Acceptor, MaybeTlsStream};
use borer_core::stream::peekable::{AsyncPeek, PeekableStream};
use borer_core::trojan::TrojanConnection;
use tracing::{Instrument, debug, error, info, info_span};

use crate::auth::AuthHandler;
use crate::config::ServerConfig;
use crate::rathole::dispatcher::Dispatcher;
use crate::setup::gen_conn_id;
use crate::store::ServerStore;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
struct State {
    acceptor: Arc<Acceptor>,
    auth_handler: Arc<AuthHandler>,
    store: Arc<ServerStore>,
}

pub async fn start_server(config: &ServerConfig) -> anyhow::Result<()> {
    let addr = &config.listen;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind server port {addr} failed"))?;
    info!(listen_addr = %addr, "server up");
    let state = create_state(config)?;

    tokio::spawn(async move {
        while let Ok((ts, _)) = listener.accept().await {
            let state = state.clone();
            let span = info_span!("connection", trace_id = %gen_conn_id());
            tokio::spawn(async move {
                if let Err(e) = server_handle(ts, state).instrument(span).await {
                    error!(error = ?e, "server connection handling failed");
                }
            });
        }
    });
    Ok(())
}

pub fn start_stats_server(config: &ServerConfig) -> anyhow::Result<()> {
    if let Some(c) = &config.traffic_stats {
        stats_server::StatsServer::new(&c.listen, &c.secret)
            .serve()
            .context("start stats server failed")?;
        info!(listen_addr = %c.listen, "stats server up");
    }
    Ok(())
}

fn create_state(config: &ServerConfig) -> anyhow::Result<State> {
    let acceptor = Acceptor::new(
        config.tls.clone().map(|o| o.cert),
        config.tls.clone().map(|o| o.key),
    )
    .context("create tls acceptor failed")?;

    let auth_handler = AuthHandler::new(&config.auth);
    let store = ServerStore::from(config);
    Ok(State {
        acceptor: Arc::new(acceptor),
        auth_handler: Arc::new(auth_handler),
        store: Arc::new(store),
    })
}

pub enum ConnType {
    Trojan(String),
    Rathole(String),
    Masquerade,
}

async fn server_handle(ts: TcpStream, state: State) -> anyhow::Result<()> {
    let peer_addr = ts.peer_addr()?;
    let ts = state.acceptor.accept(ts).await?;
    let mut peek_ts = PeekableStream::new(ts);
    debug!(peer_addr = %peer_addr, "accepted inbound connection");
    match detect_head(&mut peek_ts, &state).await? {
        ConnType::Trojan(user) => {
            TrojanConnection::new(peek_ts, user, peer_addr)
                .handle()
                .await
        }
        ConnType::Rathole(hash) => handle_rathole(peek_ts, hash, state.store).await,
        ConnType::Masquerade => MasqueradeConnection::new(peek_ts).handle().await,
    }
}

async fn detect_head(
    ts: &mut PeekableStream<MaybeTlsStream<TcpStream>>,
    state: &State,
) -> anyhow::Result<ConnType> {
    let head = proto::trojan::Request::peek_head(ts)
        .await
        .context("Server peek head failed")?;
    if head.len() < 56 {
        debug!(head_len = head.len(), "detected masquerade traffic");
        return Ok(ConnType::Masquerade);
    }

    let hash_str = String::from_utf8(head).context("Trojan hash to string failed")?;
    if let Some(u) = state.auth_handler.auth(&hash_str) {
        debug!(user = %u, "detected trojan traffic");
        Ok(ConnType::Trojan(u))
    } else if state.store.has_rathole(&hash_str) {
        debug!("detected rathole traffic");
        Ok(ConnType::Rathole(hash_str))
    } else {
        debug!("falling back to masquerade traffic");
        Ok(ConnType::Masquerade)
    }
}

async fn handle_rathole<T>(ts: T, hash: String, store: Arc<ServerStore>) -> anyhow::Result<()>
where
    T: AsyncRead + AsyncWrite + AsyncPeek + Unpin,
{
    let mut stream = ts;
    let _ = stream.drain();
    let _crlf = stream.read_u16().await?;
    let (mut dispatcher, cs) = Dispatcher::new(stream, hash.clone());
    store.set_cmd_sender(cs).await;

    dispatcher.dispatch().await.ok();

    store.remove_cmd_sender(&hash).await;
    Ok(())
}
