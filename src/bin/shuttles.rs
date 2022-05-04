use log::debug;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use shuttle::config::ServerConfig;
use shuttle::server::TlsServer;
use shuttle::socks::Socks;

#[tokio::main]
async fn main() -> shuttle::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .init();

    let config = ServerConfig::load(String::from("examples/shuttles.yaml"));
    debug!("{:?}",config);

    TlsServer::new(config.addrs[0].clone()).start().await
}
