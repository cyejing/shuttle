use std::time::Duration;

use log::info;
use shuttle::{
    init_log,
    rathole::{
        cmd::{ping::Ping, Command},
        dispatcher::{CommandRead, CommandWrite},
    },
};
use tokio::{io, net::TcpStream};

mod common;

#[tokio::test]
async fn test_rathole_ping() {
    init_log();
    common::start_command_server().await;
    common::start_server("tests/examples/shuttles.yaml").await;
    common::start_client("tests/examples/shuttlec-rathole.yaml").await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    let stream = TcpStream::connect("127.0.0.1:6788").await.unwrap();
    let (r, w) = io::split(stream);
    let mut command_write = CommandWrite::new(w);
    let mut command_read = CommandRead::new(r);
    let ping = Command::Ping(Ping::new(Some("hi".to_string())));
    command_write.write_command(10, ping).await.unwrap();

    let resp = command_read.read_command().await.unwrap();
    info!("{:?}", resp);
    assert_eq!(format!("{:?}", resp), "(10, Resp(Ok(\"hi\")))");

    info!("hi");
}
