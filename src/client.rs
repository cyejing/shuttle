use std::{sync::Arc, time::Duration};

use anyhow::Context;
use borer_core::{
    dial::{DirectDial, TrojanDial, WebSocketDial},
    proxy::ProxyConnection,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;
use tracing::{Instrument, error, info, info_span};

use crate::{
    config::{ClientConfig, ProxyMode},
    rathole,
    setup::gen_traceid,
};

pub fn start_rathole(cc: ClientConfig) -> anyhow::Result<()> {
    if cc.hole.is_some() {
        info!("run with rathole");
        let mut backoff = 400;
        tokio::spawn(async move {
            loop {
                match rathole::start_rathole(
                    &cc.server,
                    cc.get_hole_auth_hash(),
                    cc.insecure(),
                    cc.get_holes(),
                )
                .await
                {
                    Ok(()) => info!("rathole status ok"),
                    Err(e) => error!(error = ?e, remote_addr = %cc.server, "rathole failed"),
                }
                if backoff > 3200 {
                    backoff = 400;
                }
                info!(backoff_millis = backoff, "retry scheduled");
                tokio::time::sleep(Duration::from_millis(backoff)).await;

                backoff *= 2;
            }
        });
    }
    Ok(())
}

pub async fn start_proxy(cc: ClientConfig) -> anyhow::Result<()> {
    if let Some((addr_str, proxy_mode)) = cc.get_proxy() {
        info!("Run with proxy @{} use mode {:?}", addr_str, proxy_mode);
        let listener = TcpListener::bind(&addr_str)
            .await
            .with_context(|| format!("listen proxy addr {addr_str} failed"))?;

        let cc = Arc::new(cc);

        tokio::spawn(async move {
            while let Ok((ts, _)) = listener.accept().await {
                let cc = cc.clone();
                let proxy_mode = proxy_mode.clone();
                let span = info_span!("connection", trace_id = %gen_traceid(), mode = ?proxy_mode);
                tokio::spawn(
                    async move { proxy_handle(proxy_mode, cc, ts).instrument(span).await },
                );
            }
        });
    }
    Ok(())
}

async fn proxy_handle(proxy_mode: ProxyMode, cc: Arc<ClientConfig>, ts: TcpStream) {
    match proxy_mode {
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
                    cc.get_proxy_auth_hash(),
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
                    cc.get_proxy_auth_hash(),
                    cc.insecure(),
                    true,
                )),
            )
            .handle()
            .await;
        }
    }
}
