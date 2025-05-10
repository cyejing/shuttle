use std::path::PathBuf;

use clap::{Parser, Subcommand, command};
use shuttle::{
    client::{start_proxy, start_rathole},
    config::{ClientConfig, ServerConfig},
    server,
    setup::setup_log,
    store::ServerStore,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct ArgsConfig {
    // running mode
    #[command(subcommand)]
    pub mode: Mode,

    /// config path
    #[clap(short, long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Mode {
    Server,
    Client,
}

#[tokio::main]
async fn main() {
    setup_log();

    let args = ArgsConfig::parse();
    match args.mode {
        Mode::Server => {
            start_server(args).await;
        }
        Mode::Client => {
            start_client(args).await;
        }
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}

async fn start_server(args: ArgsConfig) {
    let config = ServerConfig::load(args.config);
    let store = ServerStore::from(&config);
    for addr in config.addrs {
        server::start_server(&addr, store.clone()).await;
    }
}

async fn start_client(args: ArgsConfig) {
    let cc = ClientConfig::load(args.config);
    match cc.run_type.as_str() {
        "proxy" => start_proxy(cc).await,
        "rathole" => start_rathole(cc).await,
        _ => panic!("unknown run type : {}", cc.run_type),
    }
}
