use std::path::PathBuf;

use clap::Parser;
use shuttle::config::ServerConfig;
use shuttle::logs::init_log;
use shuttle::server::start_server;
use shuttle::store::ServerStore;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Config Path
    #[clap(parse(from_os_str), name = "CONFIG_PATH")]
    config_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    init_log();

    let args: Args = Args::parse();

    let config = ServerConfig::load(args.config_path);
    let store = ServerStore::from(&config);
    for addr in config.addrs {
        start_server(addr.clone(), store.clone()).await;
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}
