use std::{sync::Arc, time::Duration};

use shuttle_station::{
    dial::{DirectDial, TrojanDial, WebSocketDial},
    proxy::ProxyConnection,
    websocket::WebSocketCopyStream,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::MaybeTlsStream;
use tracing::{info_span, Instrument};

use crate::{config::ClientConfig, gen_traceid, rathole};

pub async fn start_rathole(cc: ClientConfig) {
    info!("Run with rathole");
    let mut backoff = 400;
    tokio::spawn(async move {
        loop {
            match rathole::start_rathole(cc.clone()).await {
                Ok(_) => info!("Rathole status ok"),
                Err(e) => error!("Rathole occurs err :{:?}", e),
            }
            if backoff > 3200 {
                backoff = 400
            }
            info!("Retry after {} millis", backoff);
            tokio::time::sleep(Duration::from_millis(backoff)).await;

            backoff *= 2;
        }
    });
}

pub async fn start_proxy(cc: ClientConfig) {
    info!(
        "Run with proxy @{} use mode {}",
        cc.proxy_addr, cc.proxy_mode
    );
    let listener = TcpListener::bind(&cc.proxy_addr)
        .await
        .unwrap_or_else(|e| panic!("Can't Listen socks addr {}. err: {e}", cc.proxy_addr));

    let cc = Arc::new(cc);

    tokio::spawn(async move {
        while let Ok((ts, _)) = listener.accept().await {
            let cc = cc.clone();
            let span = info_span!("trace", traceid = gen_traceid());
            tokio::spawn(async move { proxy_handle(cc, ts).instrument(span).await });
        }
    });
}

async fn proxy_handle(cc: Arc<ClientConfig>, ts: TcpStream) {
    match (cc.proxy_mode.as_str(), cc.ssl_enable) {
        ("direct", _) => {
            ProxyConnection::<TcpStream>::new(ts, Box::<DirectDial>::default())
                .handle()
                .await;
        }
        ("trojan", false) => {
            ProxyConnection::<TcpStream>::new(
                ts,
                Box::<TrojanDial>::new(TrojanDial::new(
                    cc.remote_addr.clone(),
                    cc.hash.clone(),
                    cc.invalid_certs,
                )),
            )
            .handle()
            .await;
        }
        ("trojan", true) => {
            ProxyConnection::<TlsStream<TcpStream>>::new(
                ts,
                Box::<TrojanDial>::new(TrojanDial::new(
                    cc.remote_addr.clone(),
                    cc.hash.clone(),
                    cc.invalid_certs,
                )),
            )
            .handle()
            .await;
        }
        ("websocket", _) => {
            ProxyConnection::<WebSocketCopyStream<MaybeTlsStream<TcpStream>>>::new(
                ts,
                Box::<WebSocketDial>::new(WebSocketDial::new(
                    cc.remote_addr.clone(),
                    cc.hash.clone(),
                )),
            )
            .handle()
            .await;
        }
        _ => {
            panic!("unknown proxy_mode or ssl_enable")
        }
    };
}
