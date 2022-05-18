use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use log::info;

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
    if let Err(e) = rathole::start_rathole(cc).await {
        panic!("start rathole err :{}", e);
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
