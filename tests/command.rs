mod common;
use log::info;
use shuttle::setup::setup_log;
use tokio::io;

use shuttle::rathole::cmd::Command;
use shuttle::rathole::cmd::ping::Ping;
use shuttle::rathole::dispatcher::{CommandRead, CommandWrite};
use tokio::net::TcpStream;

#[tokio::test]
async fn test_ping() {
    setup_log();
    common::start_command_server().await;
    let stream = TcpStream::connect("127.0.0.1:6789").await.unwrap();
    let (r, w) = io::split(stream);
    let mut command_write = CommandWrite::new(w);
    let mut command_read = CommandRead::new(r);
    let ping = Command::Ping(Ping::new(Some("hi".to_string())));
    command_write.write_command(10, ping).await.unwrap();

    let resp = command_read.read_command().await.unwrap();
    info!("{:?}", resp);
    assert_eq!(format!("{:?}", resp), "(10, Resp(Ok(\"hi\")))");
}
