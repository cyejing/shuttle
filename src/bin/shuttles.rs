use std::rc::Rc;

use log::debug;

use shuttle::config::{ServerConfig, ServerStore};
use shuttle::logs::init_log;
use shuttle::server::{start_tcp_server, start_tls_server};

#[tokio::main]
async fn main() {
    init_log();

    let config = ServerConfig::load(String::from("examples/shuttles.yaml"));
    let config = Rc::new(config);

    let store = ServerStore::from(config.clone());
    debug!("{:?}",&config.addrs);
    for addr in &config.addrs {
        if addr.ssl_enable {
            tokio::spawn(start_tls_server(addr.clone(), store.clone()));
        } else {
            tokio::spawn(start_tcp_server(addr.addr.clone(), store.clone()));
        }
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}
