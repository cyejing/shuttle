pub mod frame;
pub mod session;
pub mod cmd;
pub mod parse;
pub mod shutdown;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use log::{error, info};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::mpsc;

    use crate::logs::init_log;
    use crate::rathole::session::{CmdSender, CommandRead, CommandWrite};

    #[tokio::test]
    async fn test_redis_server() {
        init_log();
        let listener = TcpListener::bind("127.0.0.1:6789").await.unwrap();
        loop {
            let (ts, _) = listener.accept().await.unwrap();
            info!("accept redis");
            let (sender, receiver) = mpsc::channel(128);
            let hash = String::from("hash");
            let cmd_sender = Arc::new(CmdSender { hash, sender });

            let (r, w) = tokio::io::split(ts);
            let command_read = CommandRead::new(r, cmd_sender);
            let command_write = CommandWrite::new(w, receiver);

            tokio::spawn(async move {
                if let Err(e) = handle(command_read, command_write).await {
                    error!("{}", e);
                }
            });
        }
    }

    async fn handle(mut command_read: CommandRead<TcpStream>,
                    mut command_write: CommandWrite<TcpStream>) -> crate::Result<()> {
        loop {
            tokio::select! {
                r = command_read.read_command() => r?,
                w = command_write.write_command() => w?,
            }
        }
    }
}
