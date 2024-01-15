use std::{sync::Arc, time::Duration};

use shuttle_station::{
    dial::{Dial, DirectDial, TrojanDial, WebSocketDial},
    proxy::ProxyConnection,
    Address,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;
use tracing::{info_span, Instrument};

use crate::{
    config::{ClientConfig, ProxyMode},
    gen_traceid, rathole,
};

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
        "Run with proxy @{} use mode {:?}",
        cc.proxy_addr, cc.proxy_mode
    );
    let listener = TcpListener::bind(&cc.proxy_addr)
        .await
        .unwrap_or_else(|e| panic!("Can't Listen socks addr {}. err: {e}", cc.proxy_addr));

    let cc = Arc::new(cc);

    if cc.proxy_mode == ProxyMode::Websocket {
        websocket_heartbeat_open(cc.remote_addr.clone(), cc.hash.clone());
    }
    tokio::spawn(async move {
        while let Ok((ts, _)) = listener.accept().await {
            let cc = cc.clone();
            let span = info_span!("trace", id = gen_traceid());
            tokio::spawn(async move { proxy_handle(cc, ts).instrument(span).await });
        }
    });
}

async fn proxy_handle(cc: Arc<ClientConfig>, ts: TcpStream) {
    match (&cc.proxy_mode, cc.ssl_enable) {
        (ProxyMode::Direct, _) => {
            ProxyConnection::new(ts, Box::<DirectDial>::default())
                .handle()
                .await;
        }
        (ProxyMode::Trojan, false) => {
            ProxyConnection::<TcpStream>::new(
                ts,
                Box::new(TrojanDial::new(
                    cc.remote_addr.clone(),
                    cc.hash.clone(),
                    cc.invalid_certs,
                )),
            )
            .handle()
            .await;
        }
        (ProxyMode::Trojan, true) => {
            ProxyConnection::<TlsStream<TcpStream>>::new(
                ts,
                Box::new(TrojanDial::new(
                    cc.remote_addr.clone(),
                    cc.hash.clone(),
                    cc.invalid_certs,
                )),
            )
            .handle()
            .await;
        }
        (ProxyMode::Websocket, _) => {
            ProxyConnection::new(
                ts,
                Box::new(WebSocketDial::new(cc.remote_addr.clone(), cc.hash.clone())),
            )
            .handle()
            .await;
        }
    };
}

fn websocket_heartbeat_open(remote_addr: String, hash: String) {
    tokio::spawn(async move {
        loop {
            let ws_dial = WebSocketDial::new(remote_addr.clone(), hash.clone());
            let domain = "www.google.com";
            let address = Address::DomainAddress(domain.as_bytes().to_vec(), 443);
            let wss = ws_dial.dial(address).await.ok();
            info!("heartbeat dial {}", wss.is_some());
            tokio::time::sleep(Duration::from_secs(30)).await
        }
    });
}
