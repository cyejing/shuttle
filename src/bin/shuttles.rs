use std::rc::Rc;

use futures::future::join_all;
use log::debug;
use tokio::join;

use shuttle::config::{ServerConfig, ServerStore};
use shuttle::logs::init_log;
use shuttle::server::{TcpServer, TlsServer};

#[tokio::main]
async fn main() {
    init_log();

    let config = ServerConfig::load(String::from("examples/shuttles.yaml"));
    let config = Rc::new(config);

    let store = ServerStore::from(config.clone());
    debug!("{:?}",&config.addrs);
    for addr in &config.addrs {
        let addr_b = addr.clone();
        let store_b = store.clone();
        if addr.ssl_enable {
            tokio::spawn(TlsServer::new(addr_b, store_b).start());
        } else {
            tokio::spawn(TcpServer::new(addr_b, store_b).start());
        }
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}
