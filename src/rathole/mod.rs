pub mod frame;
pub mod connection;
pub mod cmd;
pub mod parse;
pub mod shutdown;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use log::{error, info};
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;

    use crate::logs::init_log;
    use crate::rathole::connection::{Connection, ConnectionHolder};

    #[tokio::test]
    async fn test_redis_server() {
        init_log();
        let listener = TcpListener::bind("127.0.0.1:6789").await.unwrap();
        loop {
            let (ts, _) = listener.accept().await.unwrap();
            info!("acc");
            let (_se, re) = mpsc::channel(128);
            let mut connection_holder = ConnectionHolder::new(Connection::new(ts), re);
            tokio::spawn(async move{
                connection_holder.run().await;
            });
        }
    }
}
