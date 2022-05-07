use std::rc::Rc;

use shuttle::config::{ServerConfig, ServerStore};
use shuttle::logs::init_log;
use shuttle::server::{start_tcp_server, start_tls_server};
use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args{
    /// Config Path
    #[clap(short, long)]
    config_path: Option<String>,
}

#[tokio::main]
async fn main() {
    init_log();

    let args: Args = Args::parse();

    let config = ServerConfig::load(args.config_path);
    let config = Rc::new(config);

    let store = ServerStore::from(config.clone());
    for addr in &config.addrs {
        if addr.ssl_enable {
            tokio::spawn(start_tls_server(addr.clone(), store.clone()));
        } else {
            tokio::spawn(start_tcp_server(addr.addr.clone(), store.clone()));
        }
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}
