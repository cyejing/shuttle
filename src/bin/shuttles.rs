
use shuttle::config::ServerConfig;
use shuttle::logs::init_log;
use shuttle::server::TlsServer;

#[tokio::main]
async fn main() -> shuttle::Result<()> {
    init_log();

    let config = ServerConfig::load(String::from("examples/shuttles.yaml"));

    TlsServer::new(config.addrs[0].clone()).start().await
}
