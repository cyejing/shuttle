use std::sync::Arc;

use log::{error, info};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use shuttle::logs::init_log;
use shuttle::rathole::cmd::Command;
use shuttle::rathole::cmd::ping::Ping;
use shuttle::rathole::session::{CommandRead, CommandSender, CommandWrite};

#[tokio::test]
async fn test_ping()  {
    start_command_server().await;
    let stream = TcpStream::connect("127.0.0.1:6789").await.unwrap();
    let (mut r,mut w) = new_session(stream);
    let ping = Command::Ping(Ping::new(Some("hi".to_string())));
    let frame = ping.exec().unwrap();
    w.write_frame(&frame).await.unwrap();

    let resp = r.read_frame().await.unwrap();
    println!("{}", resp);

}

fn new_session(stream: TcpStream)-> (CommandRead<TcpStream>, CommandWrite<TcpStream>){
    let (sender, receiver) = mpsc::channel(128);
    let hash = String::from("hash");
    let cmd_sender = Arc::new(CommandSender { hash, sender });
    let (r,w) = io::split(stream);

    let command_read = CommandRead::new(r, cmd_sender);
    let command_write = CommandWrite::new(w, receiver);
    (command_read,command_write)
}


async fn start_command_server() {
    init_log();
    let listener = TcpListener::bind("127.0.0.1:6789").await.unwrap();
    tokio::spawn(async move{
        loop {
            let (ts, _) = listener.accept().await.unwrap();
            info!("accept redis");
            let (sender, receiver) = mpsc::channel(128);
            let hash = String::from("hash");
            let cmd_sender = Arc::new(CommandSender::new(hash, sender));

            let (r, w) = tokio::io::split(ts);
            let command_read = CommandRead::new(r, cmd_sender);
            let command_write = CommandWrite::new(w, receiver);

            tokio::spawn(async move {
                if let Err(e) = handle(command_read, command_write).await {
                    error!("{}", e);
                }
            });
        }
    });
}

async fn handle(mut command_read: CommandRead<TcpStream>,
                mut command_write: CommandWrite<TcpStream>) -> shuttle::Result<()> {
    loop {
        tokio::select! {
                r = command_read.read_command() => r?,
                w = command_write.write_command() => w?,
            }
    }
}