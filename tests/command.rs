use log::error;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};

use shuttle::logs::init_log;
use shuttle::rathole::cmd::ping::Ping;
use shuttle::rathole::cmd::Command;
use shuttle::rathole::dispatcher::{CommandRead, CommandWrite, Dispatcher};

#[tokio::test]
async fn test_ping() {
    start_command_server().await;
    let stream = TcpStream::connect("127.0.0.1:6789").await.unwrap();
    let (r, w) = io::split(stream);
    let mut command_write = CommandWrite::new(w);
    let mut command_read = CommandRead::new(r);
    let ping = Command::Ping(Ping::new(Some("hi".to_string())));
    command_write.write_command(10, ping).await.unwrap();

    let resp = command_read.read_command().await.unwrap();
    println!("{:?}", resp);
    assert_eq!(format!("{:?}", resp), "(10, Resp(Ok(\"hi\")))");
}

async fn start_command_server() {
    init_log();
    let listener = TcpListener::bind("127.0.0.1:6789").await.unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut dispatcher, _cs) = Dispatcher::new(stream, String::from("hash"));

            tokio::spawn(async move {
                if let Err(e) = dispatcher.dispatch().await {
                    error!("{}", e);
                }
            });
        }
    });
}
