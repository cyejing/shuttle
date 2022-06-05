use log::info;
use shuttle::proxy::{self, Dial};
use std::path::PathBuf;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use shuttle::config::{ClientConfig, ServerConfig};
use shuttle::logs::init_log;
use shuttle::store::ServerStore;

#[tokio::test]
async fn test_socks() {
    init_log();

    start_web_server().await;
    start_server().await;
    start_socks().await;

    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http("socks5://127.0.0.1:4080").unwrap())
        .build()
        .unwrap();
    let resp = client
        .get("http://127.0.0.1:6080")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(resp, "Hello world!");
    info!("assert eq {}", resp);
}

async fn start_server() {
    let config = ServerConfig::load(Option::Some(PathBuf::from("tests/examples/shuttles.yaml")));

    let store = ServerStore::from(&config);
    let addr = config.addrs.get(0).unwrap();
    shuttle::server::start_server(addr.clone(), store.clone()).await;
}

async fn start_socks() {
    let cc = ClientConfig::load(Option::Some(PathBuf::from("tests/examples/shuttlec.yaml")));

    let dial = Dial::Trojan(cc.remote_addr.clone(), cc.hash.clone(), cc.ssl_enable);
    proxy::start_proxy(&cc.proxy_addr, dial).await;
}

async fn start_web_server() {
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
