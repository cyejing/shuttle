use std::time::Duration;

use crate::{config::ClientConfig, proxy, rathole};

pub async fn start_rathole(cc: ClientConfig) {
    info!("run with rathole");
    let mut backoff = 100;

    tokio::spawn(async move {
        loop {
            match rathole::start_rathole(cc.clone()).await {
                Ok(_) => info!("rathole ok ?"),
                Err(e) => error!("Rathole occurs err :{:?}", e),
            }
            if backoff > 6400 {
                backoff = 1200
            }
            info!("Retry after {} millis", backoff);
            tokio::time::sleep(Duration::from_millis(backoff)).await;

            backoff *= 20;
        }
    });
}

pub async fn start_proxy(cc: ClientConfig, mode: String) {
    info!("run with socks");
    match mode.as_str() {
        "trojan" => {
            let dial = proxy::Dial::Trojan(cc.remote_addr.clone(), cc.hash.clone(), cc.ssl_enable);
            proxy::start_proxy(&cc.proxy_addr, dial).await;
        }
        "direct" => {
            let dial = proxy::Dial::Direct;
            proxy::start_proxy(&cc.proxy_addr, dial).await;
        }
        _ => panic!("unknown socks mode"),
    }
}
