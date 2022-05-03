use shuttle::config::ServerConfig;
use shuttle::server::tls_server::{TlsServer};

#[tokio::main]
async fn main() {
    let config = ServerConfig::load(String::from("res/shuttles.yaml"));
    let server = TlsServer::new(config.addrs[0].clone());

    server.start().await;
}
