use anyhow::Context;
use log::error;
use shuttle::init_log;
use shuttle_core::{dial::DirectDial, proxy::ProxyConnection};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() {
    init_log();

    let listener = TcpListener::bind("127.0.0.1:4080")
        .await
        .context("Can't Listen socks addr ")
        .unwrap();

    while let Ok((ts, _)) = listener.accept().await {
        tokio::spawn(async move {
            let dial = Box::<DirectDial>::default();
            let mut conn = ProxyConnection::<TcpStream>::new(ts, dial);
            if let Err(e) = conn.handle().await {
                error!("ProxyServer handle stream err: {e:?}");
            }
        });
    }
}
