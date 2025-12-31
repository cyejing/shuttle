use std::sync::Arc;

use anyhow::Context;
use borer_core::masquerade::MasqueradeConnection;
use borer_core::proto::{self};
use borer_core::stats_server;
use borer_core::stream::acceptor::{Acceptor, MaybeTlsStream};
use borer_core::stream::peekable::{AsyncPeek, PeekableStream};
use borer_core::trojan::TrojanConnection;
use tracing::{Instrument, info_span};

use crate::auth::AuthHandler;
use crate::config::ServerConfig;
use crate::rathole::dispatcher::Dispatcher;
use crate::setup::gen_traceid;
use crate::store::ServerStore;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
struct State {
    acceptor: Arc<Acceptor>,
    auth_handler: Arc<AuthHandler>,
    store: Arc<ServerStore>,
}

pub async fn start_server(config: &ServerConfig) {
    let addr = &config.listen;
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|_| panic!("Can't bind server port {addr}"));
    info!("Server up and running. listen: {addr}");
    let state = create_state(config);

    tokio::spawn(async move {
        while let Ok((ts, _)) = listener.accept().await {
            let state = state.clone();
            let span = info_span!("trace", id = gen_traceid());
            tokio::spawn(async move {
                if let Err(e) = server_handle(ts, state).instrument(span).await {
                    error!("Server handle failed. err: {e:?}");
                }
            });
        }
    });
}

pub fn start_stats_server(config: &ServerConfig) {
    if let Some(c) = &config.traffic_stats {
        stats_server::StatsServer::new(&c.listen, &c.secret).serve();
        info!("stats_server up and running. listen: {}", &c.listen);
    }
}

fn create_state(config: &ServerConfig) -> State {
    let acceptor = Acceptor::new(
        config.tls.clone().map(|o| o.cert),
        config.tls.clone().map(|o| o.key),
    );

    let auth_handler = AuthHandler::new(&config.auth);
    let store = ServerStore::from(config);
    State {
        acceptor: Arc::new(acceptor),
        auth_handler: Arc::new(auth_handler),
        store: Arc::new(store),
    }
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
        debug!("detect is masquerade");
        return Ok(ConnType::Masquerade);
    }

    let hash_str = String::from_utf8(head).context("Trojan hash to string failed")?;
    if let Some(u) = state.auth_handler.auth(&hash_str).await {
        debug!("detect is trojan");
        Ok(ConnType::Trojan(u))
    } else if state.store.has_rathole(&hash_str) {
        debug!("detect is rathole");
        Ok(ConnType::Rathole(hash_str))
    } else {
        debug!("detect is masquerade");
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
