mod common;
use log::info;

use shuttle::init_log;

#[tokio::test]
async fn test_socks() {
    init_log();

    common::start_web_server().await;
    common::start_server("tests/examples/shuttles.yaml").await;
    common::start_client("tests/examples/shuttlec-proxy.yaml").await;

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
    info!("assert eq {}", resp);
}
