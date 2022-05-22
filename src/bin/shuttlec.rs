use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use log::{error, info};

use shuttle::config::ClientConfig;
use shuttle::logs::init_log;
use shuttle::socks::TrojanDial;
use shuttle::{rathole, socks};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Config Path
    #[clap(short, long)]
    config_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    init_log();

    let args: Args = Args::parse();

    let cc = ClientConfig::load(args.config_path);
    match cc.run_type.as_str() {
        "socks" => start_socks(cc).await,
        "rathole" => start_rathole(cc).await,
        _ => panic!("unknown run type : {}", cc.run_type),
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}

async fn start_rathole(cc: ClientConfig) {
    info!("run with rathole");
    let mut backoff = 100;

    loop {
        if let Err(e) = rathole::start_rathole(cc.clone()).await {
            error!("Rathole occurs err :{:?}", e);
            if backoff > 6400 {
                backoff = 1200
            }
        }
        info!("Retry after {} millis", backoff);
        tokio::time::sleep(Duration::from_millis(backoff)).await;

        backoff *= 2;
    }
}

async fn start_socks(cc: ClientConfig) {
    info!("run with socks");
    let dial = Arc::new(TrojanDial::new(
        cc.remote_addr.clone(),
        cc.hash.clone(),
        cc.ssl_enable,
    ));
    socks::start_socks(cc, dial).await;
}
