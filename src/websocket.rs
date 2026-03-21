use std::{io::Cursor, net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use borer_core::{
    dial::{Dial as _, DirectDial},
    proto::{padding::Padding, trojan},
    store::store,
    stream::{stats::StatsStream, websocket::AxumWebSocketCopyStream},
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::{
    io::{AsyncWriteExt, copy_bidirectional},
    time::timeout,
};
use tracing::{Instrument, debug, error, info, info_span, warn};

use crate::{auth::AuthHandler, config::ServerConfig, setup::gen_conn_id};
use axum_server::tls_rustls::RustlsConfig;

#[derive(Clone)]
struct AppState {
    auth_handler: Arc<AuthHandler>,
}

pub async fn start_websocket(addr: &str, config: &ServerConfig) -> anyhow::Result<()> {
    let state = create_state(config);
    let router = Router::new()
        .route("/clients", get(websocket_handler))
        .route("/fly", get(fly_handler))
        .route("/stat", get(html_handler))
        .with_state(state);

    let tls_config = match &config.tls {
        Some(tls) => {
            let tc = RustlsConfig::from_pem_file(&tls.cert, &tls.key)
                .await
                .context("load websocket tls files failed")?;
            Some(tc)
        }
        None => None,
    };
    let addr: SocketAddr = addr
        .parse()
        .with_context(|| format!("parse websocket listen addr {addr} failed"))?;

    tokio::spawn(async move {
        let serve_result = match tls_config {
            Some(tc) => axum_server::bind_rustls(addr, tc)
                .serve(router.into_make_service())
                .await
                .map_err(anyhow::Error::from),
            None => axum_server::bind(addr)
                .serve(router.into_make_service())
                .await
                .map_err(anyhow::Error::from),
        };
        if let Err(error) = serve_result {
            error!(listen_addr = %addr, error = ?error, "websocket server stopped");
        }
    });
    info!(listen_addr = %addr, "websocket up");
    Ok(())
}

fn create_state(config: &ServerConfig) -> AppState {
    let auth_handler = AuthHandler::new(&config.auth);
    AppState {
        auth_handler: Arc::new(auth_handler),
    }
}

async fn fly_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    let span = info_span!("connection", trace_id = %gen_conn_id(), transport = "websocket");
    ws.on_upgrade(|socket| fly(socket, state).instrument(span))
}

async fn fly(mut stream: WebSocket, state: AppState) {
    if let Some(Ok(Message::Binary(buf))) = stream.recv().await {
        let mut r = Cursor::new(buf);
        match trojan::Request::read_from(&mut r).await {
            Ok(req) => {
                let addr = req.address.clone();
                if let Some(user) = state.auth_handler.auth(&req.hash) {
                    let padding = req.is_padding();
                    if padding {
                        let buf = Padding::default().rand_buf();
                        let _ = stream.send(Message::Binary(Bytes::from(buf))).await;
                    }

                    info!(padding, target_addr = %addr, user = %user, "trojan websocket connect");

                    let ws = AxumWebSocketCopyStream::new(stream);
                    let mut stats_stream = StatsStream::new(
                        ws,
                        user,
                        req.hash,
                        "unknown".to_string(),
                        addr.to_string(),
                        padding,
                    );

                    match DirectDial::new(Duration::from_secs(10))
                        .dial(addr.clone())
                        .await
                    {
                        Ok(mut remote_ts) => {
                            timeout(Duration::from_secs(3), remote_ts.write_buf(&mut r))
                                .await
                                .ok();
                            if let Ok((a, b)) =
                                copy_bidirectional(&mut stats_stream, &mut remote_ts).await
                            {
                                debug!(target_addr = %req.address, upstream_bytes = a, downstream_bytes = b, total_bytes = a + b, "trojan websocket copy completed");
                            }
                        }
                        Err(e) => {
                            warn!(error = ?e, target_addr = %addr, "websocket dial remote failed");
                        }
                    }
                } else {
                    error!(target_addr = %addr, "trojan websocket auth failed");
                    let _ = stream.send(Message::Close(None)).await;
                }
            }
            Err(e) => {
                warn!(error = %e, "trojan websocket request read failed");
            }
        }
    }
}

async fn websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(websocket)
}

#[derive(Serialize)]
struct StatResponse {
    up_count: usize,
    down_count: usize,
    conn_count: usize,
    #[serde(rename = "dateTime")]
    date_time: DateTime<Utc>,
    is_up: bool,
}

async fn websocket(stream: WebSocket) {
    // By splitting we can send and receive at the same time.
    let (mut sender, mut receiver) = stream.split();

    loop {
        match receiver.next().await {
            Some(Ok(Message::Text(_text))) => {
                let (up_count, down_count, conn_count) = store()
                    .get_traffic_all()
                    .values()
                    .map(|s| (s.up, s.down, s.conn_count))
                    .reduce(|(u, d, c), (u1, d1, c1)| (u + u1, d + d1, c + c1))
                    .unwrap_or_default();

                let response = StatResponse {
                    date_time: Utc::now(),
                    is_up: true,
                    up_count,
                    down_count,
                    conn_count,
                };
                let msg = serde_json::to_string(&response).unwrap();
                if sender.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            Some(Err(e)) => {
                debug!(error = %e, "websocket client stream error");
                break;
            }
            None => {
                debug!("websocket client stream closed");
                break;
            }
            _ => {}
        }
    }
}

#[derive(rust_embed::RustEmbed)]
#[folder = "static"]
struct Assets;

async fn html_handler() -> impl IntoResponse {
    let path = "index.html";
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}
