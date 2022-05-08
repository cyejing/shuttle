pub mod frame;
pub mod connection;

#[cfg(test)]
mod tests {
    use std::io;
    use std::time::Duration;

    use bytes::{Buf, BytesMut};
    use log::{error, info};
    use tokio::io::{AsyncReadExt, BufWriter};
    use tokio::net::TcpListener;

    use crate::rathole::connection::Connection;
    use crate::logs::init_log;

    #[tokio::test]
    async fn test_redis_server() {
        init_log();
        let listener = TcpListener::bind("127.0.0.1:6789").await.unwrap();
        loop {
            let (ts, _) = listener.accept().await.unwrap();
            let mut connection = Connection::new(ts);
            tokio::spawn(async move {
                loop {
                    match connection.read_frame().await {
                        Ok(f) => {
                            info!("{:?}", f);
                        }
                        Err(e) => {
                            error!("{}",e);
                            panic!("{}", e);
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            });
        }
    }
}
