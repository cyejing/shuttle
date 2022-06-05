use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use log::{error, info};

use shuttle::config::ClientConfig;
use shuttle::logs::init_log;
use shuttle::{proxy, rathole};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Config Path
    #[clap(parse(from_os_str), name = "CONFIG_PATH")]
    config_path: Option<PathBuf>,
    #[clap(short, long, default_value = "trojan")]
    proxy_mode: String,
}

#[tokio::main]
async fn main() {
    init_log();

    let args: Args = Args::parse();

    let cc = ClientConfig::load(args.config_path);
    match cc.run_type.as_str() {
        "proxy" => start_proxy(cc, args.proxy_mode).await,
        "rathole" => start_rathole(cc).await,
        _ => panic!("unknown run type : {}", cc.run_type),
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}

async fn start_rathole(cc: ClientConfig) {
    info!("run with rathole");
    let mut backoff = 100;

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
}

async fn start_proxy(cc: ClientConfig, mode: String) {
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
