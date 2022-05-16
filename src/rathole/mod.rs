use bytes::{Bytes, BytesMut};
use log::info;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use crate::common::consts;

use crate::config::ClientConfig;
use crate::rathole::cmd::exchange::Exchange;
use crate::rathole::cmd::proxy::Proxy;
use crate::rathole::cmd::Command;
use crate::rathole::context::Context;
use crate::rathole::dispatcher::Dispatcher;

pub mod cmd;
pub mod context;
pub mod dispatcher;
pub mod frame;

pub type ReqChannel = Option<oneshot::Sender<crate::Result<()>>>;
pub type CommandChannel = (u64, Command, ReqChannel);

pub async fn start_rathole(cc: ClientConfig) -> crate::Result<()> {
    let mut stream = TcpStream::connect(cc.remote_addr)
        .await
        .expect("connect remote addr err");

    let mut buf: Vec<u8> = vec![];
    buf.extend_from_slice(cc.hash.as_bytes());
    buf.extend_from_slice(&consts::CRLF);
    stream.write_all(buf.as_slice()).await?;

    let mut dispatcher = Dispatcher::new(stream, cc.hash);
    let command_sender = dispatcher.get_command_sender();


    let f = tokio::spawn(async move { dispatcher.dispatch().await });

    for hole in cc.holes {
        let open_proxy = Command::Proxy(Proxy::new(
            hole.remote_addr.clone(),
            hole.local_addr.clone(),
        ));
        command_sender.send_sync(open_proxy).await?;
        info!(
            "open proxy for [remote:{}], [local:{}]",
            hole.remote_addr, hole.local_addr
        );
    }

    f.await?
}

async fn exchange_copy(
    ts: TcpStream,
    mut rx: mpsc::Receiver<Bytes>,
    context: Context,
) -> crate::Result<()> {
    let (mut r, mut w) = io::split(ts);
    info!("satrt stream copy by exchange conn_id: {:?}",context.current_conn_id);
    loop {
        tokio::select! {
            r1 = read_bytes(&mut r, context.clone()) => r1?,
            r2 = write_bytes(&mut w, &mut rx) => r2?,
        }
    }
}

async fn read_bytes(r: &mut ReadHalf<TcpStream>, context: Context) -> crate::Result<()> {
    let mut buf = BytesMut::new();
    let len = r.read_buf(&mut buf).await?;
    if len > 0 {
        info!("read byte len:{}", len);
        let exchange = Command::Exchange(Exchange::new(context.get_conn_id(), buf.freeze()));
        context.command_sender.send(exchange).await?;
        Ok(())
    } else {
        Err("proxy conn EOF".into())
    }
}

async fn write_bytes(
    w: &mut WriteHalf<TcpStream>,
    rx: &mut mpsc::Receiver<Bytes>,
) -> crate::Result<()> {
    if let Some(bytes) = rx.recv().await {
        w.write_all(&bytes).await?;
        Ok(())
    } else {
        Err("exchange receiver none".into())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Cursor;

    use crate::rathole::cmd::ping::Ping;
    use crate::rathole::cmd::resp::Resp;
    use crate::rathole::cmd::Command;
    use crate::rathole::dispatcher::{CommandRead, CommandWrite};

    pub fn new_command_read(buf: &mut Vec<u8>) -> CommandRead<Cursor<Vec<u8>>> {
        let (r, _w) = tokio::io::split(Cursor::new(Vec::from(buf.as_slice().clone())));
        CommandRead::new(r)
    }

    pub fn new_command_write(buf: &mut Vec<u8>) -> CommandWrite<Cursor<&mut Vec<u8>>> {
        let (_r, w) = tokio::io::split(Cursor::new(buf));
        CommandWrite::new(w)
    }

    async fn command_write_read(cmd: Command) -> Command {
        let mut cell = RefCell::new(Vec::new());
        let mut command_write = new_command_write(cell.get_mut());
        command_write.write_command(10, cmd).await.unwrap();
        let mut command_read = new_command_read(cell.get_mut());
        let (req_id, cmd) = command_read.read_command().await.unwrap();
        assert_eq!(10, req_id);
        cmd
    }

    #[tokio::test]
    pub async fn test_command_ping() {
        let ping = Command::Ping(Ping::default());
        let cmd = command_write_read(ping).await;
        assert_eq!("Ping(Ping { msg: Some(\"pong\") })", format!("{:?}", cmd));
    }

    #[tokio::test]
    pub async fn test_command_resp() {
        let resp = Command::Resp(Resp::Ok("hi".to_string()));
        let cmd = command_write_read(resp).await;
        assert_eq!("Resp(Ok(\"hi\"))", format!("{:?}", cmd));
    }

    #[tokio::test]
    pub async fn test_command_resp_err() {
        let resp = Command::Resp(Resp::Err("hi".to_string()));
        let cmd = command_write_read(resp).await;
        assert_eq!("Resp(Err(\"hi\"))", format!("{:?}", cmd));
    }
}
