use std::path::PathBuf;
use std::time::Duration;

use log::info;
use shuttle::{
    client::{start_proxy, start_rathole},
    config::{self},
    rathole::dispatcher::Dispatcher,
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::OnceCell,
};

static COMMAND_SERVER: OnceCell<()> = OnceCell::const_new();
static WEB_SERVER: OnceCell<()> = OnceCell::const_new();
static SHUTTLE_SERVER: OnceCell<()> = OnceCell::const_new();
static PROXY_CLIENT: OnceCell<()> = OnceCell::const_new();
static RATHOLE_CLIENT: OnceCell<()> = OnceCell::const_new();

#[allow(dead_code)]
pub async fn start_command_server() {
    COMMAND_SERVER
        .get_or_init(|| async {
            let listener = TcpListener::bind("127.0.0.1:6789").await.unwrap();
            tokio::spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    info!("command server accept req");
                    let (mut dispatcher, _cs) = Dispatcher::new(stream, String::from("hash"));

                    tokio::spawn(async move {
                        dispatcher.dispatch().await.ok();
                    });
                }
            });
        })
        .await;
    wait_for_port("127.0.0.1:6789").await;
}

#[allow(dead_code)]
pub async fn start_server(path: &str) {
    SHUTTLE_SERVER
        .get_or_init(|| async move {
            let config = config::load_server_config(Option::Some(PathBuf::from(path)));
            shuttle::server::start_server(&config)
                .await
                .expect("start test server failed");
        })
        .await;
    wait_for_port("127.0.0.1:4982").await;
}

#[allow(dead_code)]
pub async fn start_client(t: &str, path: &str) {
    match t {
        "proxy" => {
            PROXY_CLIENT
                .get_or_init(|| async move {
                    let cc = config::load_client_config(Option::Some(PathBuf::from(path)));
                    start_proxy(cc).await.expect("start proxy client failed");
                })
                .await;
            wait_for_port("127.0.0.1:4082").await;
        }
        "rathole" => {
            RATHOLE_CLIENT
                .get_or_init(|| async move {
                    let cc = config::load_client_config(Option::Some(PathBuf::from(path)));
                    start_rathole(cc).expect("start rathole client failed");
                })
                .await;
            wait_for_port("127.0.0.1:6788").await;
        }
        _ => panic!("unknown run type : {t}"),
    };
}

#[allow(dead_code)]
pub async fn start_web_server() {
    WEB_SERVER
        .get_or_init(|| async {
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
        })
        .await;
    wait_for_port("127.0.0.1:6080").await;
}

async fn wait_for_port(addr: &str) {
    for _ in 0..50 {
        if TcpStream::connect(addr).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {addr}");
}
