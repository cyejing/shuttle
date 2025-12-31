use std::path::PathBuf;

use log::{error, info};
use shuttle::{
    client::{start_proxy, start_rathole},
    config::{self},
    rathole::dispatcher::Dispatcher,
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
    let config = config::load_server_config(Option::Some(PathBuf::from(path)));
    shuttle::server::start_server(&config).await;
}

#[allow(dead_code)]
pub async fn start_client(t: &str, path: &str) {
    let cc = config::load_client_config(Option::Some(PathBuf::from(path)));
    match t {
        "proxy" => start_proxy(cc).await,
        "rathole" => start_rathole(cc).await,
        _ => panic!("unknown run type : {t}"),
    };
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
