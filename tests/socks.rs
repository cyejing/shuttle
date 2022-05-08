use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use log::info;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use shuttle::config::{ClientConfig, ServerConfig, ServerStore};
use shuttle::logs::init_log;
use shuttle::server::start_tcp_server;
use shuttle::socks::{Socks, TrojanDial};

#[tokio::test]
async fn test_socks() {
    init_log();
    start_server();
    start_socks();
    start_web_server().await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http("socks5://127.0.0.1:4080").unwrap())
        .build().unwrap();
    let resp = client.get("http://127.0.0.1:6080").send()
        .await.unwrap()
        .text()
        .await.unwrap();
    assert_eq!(resp, "Hello world!");
    info!("assert eq {}",resp);
}

fn start_server() {
    let config = ServerConfig::load(Option::Some(String::from("tests/examples/shuttles.yaml")));
    let store = ServerStore::from(Rc::new(config));

    tokio::spawn(start_tcp_server(String::from("127.0.0.1:4880"), store.clone()));
}

fn start_socks() {
    let cc = ClientConfig::load(Option::Some(String::from("tests/examples/shuttlec.yaml")));

    let dial = Arc::new(TrojanDial::new(cc.remote_addr.clone(),
                                        cc.hash.clone(),
                                        cc.ssl_enable));
    tokio::spawn(Socks::new(cc, dial).start());
}

async fn start_web_server() {
    let listener = TcpListener::bind("127.0.0.1:6080").await.unwrap();
    info!("Listener web server 8080");
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
            ).await.unwrap();
        }
    });
}
