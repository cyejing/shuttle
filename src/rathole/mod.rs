pub mod frame;
pub mod connection;
pub mod cmd;
pub mod parse;
pub mod shutdown;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use log::{error, info};
    use tokio::net::TcpListener;
    use tokio::sync::{broadcast, mpsc};
    use tokio::sync::broadcast::Sender;

    use crate::logs::init_log;
    use crate::rathole::connection::{CmdSender, Connection, ConnectionHolder};

    #[tokio::test]
    async fn test_redis_server() {
        init_log();
        let listener = TcpListener::bind("127.0.0.1:6789").await.unwrap();
        loop {
            let (ts, _) = listener.accept().await.unwrap();
            info!("acc");
            let hash = String::from("hash");
            let (sender, re) = mpsc::channel(128);
            let cmd_sender = Arc::new(CmdSender { hash, sender });

            let mut connection_holder = ConnectionHolder::new(
                Connection::new(ts), re,cmd_sender);
            tokio::spawn(async move {
                if let Err(e) = connection_holder.run().await {
                    error!("err : {:?}",e);
                }
            });
        }
    }

}
