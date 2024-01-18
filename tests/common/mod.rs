use std::path::PathBuf;

use log::{error, info};
use shuttle::{
    client::{start_proxy, start_rathole},
    config::{ClientConfig, ServerConfig},
    rathole::dispatcher::Dispatcher,
    store::ServerStore,
};
use tokio::{io::AsyncWriteExt, net::TcpListener};

#[allow(dead_code)]
pub async fn start_command_server() {
    let listener = TcpListener::bind("127.0.0.1:6789").await.unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            info!("command server accept req");
            let (mut dispatcher, _cs) = Dispatcher::new(stream, String::from("hash"));

            tokio::spawn(async move {
                if let Err(e) = dispatcher.dispatch().await {
                    error!("{:?}", e);
                }
            });
        }
    });
}

#[allow(dead_code)]
pub async fn start_server(path: &str) {
    let config = ServerConfig::load(Option::Some(PathBuf::from(path)));

    let store = ServerStore::from(&config);
    let addr = config.addrs.get(0).unwrap();
    shuttle::server::start_server(&addr, store.clone()).await;
}

#[allow(dead_code)]
pub async fn start_client(path: &str) {
    let cc = ClientConfig::load(Option::Some(PathBuf::from(path)));
    match cc.run_type.as_str() {
        "proxy" => start_proxy(cc).await,
        "rathole" => start_rathole(cc).await,
        _ => panic!("unknown run type : {}", cc.run_type),
    }
}

#[allow(dead_code)]
pub async fn start_web_server() {
    let listener = TcpListener::bind("127.0.0.1:6080").await.unwrap();
    info!("Listener web server 6080");
    tokio::spawn(async move {
        loop {
            let (mut ts, _sa) = listener.accept().await.unwrap();
            ts.write_all(
                &b"HTTP/1.0 200 ok\r\n\
                    Connection: keep-alive\r\n\
                    Content-Type: text/plain; charset=utf-8\r\n\
                    Content-length: 12\r\n\
                    \r\n\
                    Hello world!"[..],
            )
            .await
            .unwrap();
        }
    });
}
