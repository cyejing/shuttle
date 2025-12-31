use std::{io::Cursor, net::SocketAddr, sync::Arc, time::Duration};

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

use crate::{auth::AuthHandler, config::ServerConfig, setup::gen_traceid};
use axum_server::tls_rustls::RustlsConfig;

#[derive(Clone)]
struct AppState {
    auth_handler: Arc<AuthHandler>,
}

pub async fn start_websocket(addr: &str, config: &ServerConfig) {
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
                .expect("load ssl file failed");
            Some(tc)
        }
        None => None,
    };
    let addr: SocketAddr = addr
        .parse()
        .unwrap_or_else(|_| panic!("addr parse failed {addr}"));

    tokio::spawn(async move {
        match tls_config {
            Some(tc) => {
                axum_server::bind_rustls(addr, tc)
                    .serve(router.into_make_service())
                    .await
                    .unwrap_or_else(|_| panic!("Can't bind websocket port {addr}"));
            }
            None => {
                axum_server::bind(addr)
                    .serve(router.into_make_service())
                    .await
                    .unwrap_or_else(|_| panic!("Can't bind websocket port {addr}"));
            }
        }
    });
    info!("websocket up and running. listen: {addr}");
}

fn create_state(config: &ServerConfig) -> AppState {
    let auth_handler = AuthHandler::new(&config.auth);
    AppState {
        auth_handler: Arc::new(auth_handler),
    }
}

async fn fly_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    let span = info_span!("trace", id = gen_traceid());
    ws.on_upgrade(|socket| fly(socket, state).instrument(span))
}

async fn fly(mut stream: WebSocket, state: AppState) {
    if let Some(Ok(Message::Binary(buf))) = stream.recv().await {
        let mut r = Cursor::new(buf);
        match trojan::Request::read_from(&mut r).await {
            Ok(req) => {
                let addr = req.address.clone();
                if let Some(user) = state.auth_handler.auth(&req.hash).await {
                    let padding = req.is_padding();
                    if padding {
                        let buf = Padding::default().rand_buf();
                        let _ = stream.send(Message::Binary(Bytes::from(buf))).await;
                    }

                    info!("[{padding}] Trojan Connect to {addr}. ",);

                    let ws = AxumWebSocketCopyStream::new(stream);
                    let mut stats_stream = StatsStream::new(
                        ws,
                        user,
                        req.hash,
                        "unknown".to_string(),
                        addr.to_string(),
                        padding,
                    );

                    match DirectDial::default().dial(addr).await {
                        Ok(mut remote_ts) => {
                            timeout(Duration::from_secs(3), remote_ts.write_buf(&mut r))
                                .await
                                .ok();
                            if let Ok((a, b)) =
                                copy_bidirectional(&mut stats_stream, &mut remote_ts).await
                            {
                                debug!(
                                    "trojan copy end for {} traffic: {}<=>{} total: {}",
                                    req.address,
                                    a,
                                    b,
                                    a + b
                                );
                            }
                        }
                        Err(e) => {
                            warn!("dial remote failed. err: {}", e);
                        }
                    }
                } else {
                    error!("trojan requested to {}. hash nomatch", addr);
                    let _ = stream.send(Message::Close(None)).await;
                }
            }
            Err(e) => {
                warn!("trojan request read failed. err: {}", e)
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
                debug!("websocket clients occured {e}");
                break;
            }
            None => {
                debug!("websocket next is none");
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
