mod common;

use log::info;
use shuttle::setup::setup_log;
use std::path::PathBuf;

#[tokio::test]
async fn test_socks() {
    let _ = setup_log(PathBuf::from("logs"));

    common::start_web_server().await;
    common::start_server("tests/examples/server.yaml").await;
    common::start_client("proxy", "tests/examples/client-proxy.yaml").await;

    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http("socks5://127.0.0.1:4082").unwrap())
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
    info!("assert eq {resp}");
}
