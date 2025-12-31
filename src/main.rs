use clap::Parser;
use shuttle::{
    client::{start_proxy, start_rathole},
    config::{self, ArgsConfig, Mode},
    server::{start_server, start_stats_server},
    setup::setup_log,
    websocket::start_websocket,
};

#[tokio::main]
async fn main() {
    setup_log();
    let args = ArgsConfig::parse();

    match args.mode {
        Mode::Server => {
            let config = config::load_server_config(args.config);
            start_server(&config).await;
            start_stats_server(&config);
            if let Some(ws) = &config.websocket {
                start_websocket(&ws.listen, &config).await;
            }
        }
        Mode::Client => {
            let config = config::load_client_config(args.config);
            start_rathole(config.clone()).await;
            start_proxy(config).await;
        }
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}
