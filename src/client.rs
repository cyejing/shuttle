use std::{sync::Arc, time::Duration};

use borer_core::{
    dial::{DirectDial, TrojanDial, WebSocketDial},
    proxy::ProxyConnection,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;
use tracing::{Instrument, info_span};

use crate::{
    auth::sha224,
    config::{ClientConfig, ProxyMode},
    rathole,
    setup::gen_traceid,
};

pub async fn start_rathole(cc: ClientConfig) {
    info!("Run with rathole");
    let mut backoff = 400;
    tokio::spawn(async move {
        loop {
            match rathole::start_rathole(&cc.server, cc.insecure(), sha224(&cc.auth), &cc.holes)
                .await
            {
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
    let addr_str = &cc.proxy.listen;
    let proxy_mode = &cc.proxy.mode;
    info!("Run with proxy @{} use mode {:?}", addr_str, proxy_mode);
    let listener = TcpListener::bind(&addr_str)
        .await
        .unwrap_or_else(|e| panic!("Can't Listen socks addr {}. err: {e}", addr_str));

    let cc = Arc::new(cc);

    tokio::spawn(async move {
        while let Ok((ts, _)) = listener.accept().await {
            let cc = cc.clone();
            let span = info_span!("trace", id = gen_traceid());
            tokio::spawn(async move { proxy_handle(cc, ts).instrument(span).await });
        }
    });
}

async fn proxy_handle(cc: Arc<ClientConfig>, ts: TcpStream) {
    match &cc.proxy.mode {
        ProxyMode::Direct => {
            ProxyConnection::new(ts, Box::<DirectDial>::default())
                .handle()
                .await;
        }

        ProxyMode::Trojan => {
            ProxyConnection::<TlsStream<TcpStream>>::new(
                ts,
                Box::new(TrojanDial::new(
                    cc.server.clone(),
                    sha224(&cc.auth),
                    cc.insecure(),
                    true,
                )),
            )
            .handle()
            .await;
        }
        ProxyMode::Websocket => {
            ProxyConnection::new(
                ts,
                Box::new(WebSocketDial::new(
                    cc.server.clone(),
                    sha224(&cc.auth),
                    true,
                )),
            )
            .handle()
            .await;
        }
    };
}
