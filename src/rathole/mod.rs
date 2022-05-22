use anyhow::{anyhow, Context};
use bytes::{Bytes, BytesMut};
use std::sync::Arc;
use std::time::Duration;
use tokio::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

use crate::config::ClientConfig;
use crate::rathole::cmd::exchange::Exchange;
use crate::rathole::cmd::ping::Ping;
use crate::rathole::cmd::proxy::Proxy;
use crate::rathole::cmd::Command;
use crate::rathole::context::CommandSender;
use crate::rathole::dispatcher::Dispatcher;
use crate::tls::{make_server_name, make_tls_connector};
use crate::CRLF;

pub mod cmd;
pub mod context;
pub mod dispatcher;
pub mod frame;

pub type ReqChannel = Option<oneshot::Sender<anyhow::Result<()>>>;
pub type CommandChannel = (u64, Command, ReqChannel);

pub async fn start_rathole(cc: ClientConfig) -> anyhow::Result<()> {
    let remote_addr = &cc.remote_addr;
    let stream = TcpStream::connect(remote_addr)
        .await
        .context(format!("Can't connect remote addr {}", remote_addr))?;

    if cc.ssl_enable {
        let domain = make_server_name(remote_addr)?;
        let tls_stream = make_tls_connector().connect(domain, stream).await?;
        handle(tls_stream, cc).await
    } else {
        handle(stream, cc).await
    }
}

async fn handle<T: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut stream: T,
    cc: ClientConfig,
) -> anyhow::Result<()> {
    let mut buf: Vec<u8> = vec![];
    buf.extend_from_slice(cc.hash.as_bytes());
    buf.extend_from_slice(&CRLF);
    stream
        .write_all(buf.as_slice())
        .await
        .context("Can't write rathole hash")?;

    let (mut dispatcher, command_sender) = Dispatcher::new(stream, cc.hash);

    let dispatch =
        tokio::spawn(async move { dispatcher.dispatch().await.context("Rathole dispatch end") });

    let hcs = command_sender.clone();
    let heartbeat =
        tokio::spawn(async move { heartbeat(hcs).await.context("Break for heartbeat") });

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

    tokio::select!(
        r = dispatch => r?,
        r2 = heartbeat => r2?,
    )
}

async fn heartbeat(command_sender: Arc<CommandSender>) -> anyhow::Result<()> {
    loop {
        command_sender.send(Command::Ping(Ping::new(None))).await?;
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

async fn exchange_copy(
    ts: TcpStream,
    mut rx: mpsc::Receiver<Bytes>,
    context: context::Context,
) -> anyhow::Result<()> {
    let (mut r, mut w) = io::split(ts);
    info!(
        "start stream copy by exchange conn_id: {:?}",
        context.current_conn_id
    );
    loop {
        tokio::select! {
            r1 = read_bytes(&mut r, context.clone()) => r1?,
            r2 = write_bytes(&mut w, &mut rx) => r2?,
        }
    }
}

async fn read_bytes(r: &mut ReadHalf<TcpStream>, context: context::Context) -> anyhow::Result<()> {
    let mut buf = BytesMut::with_capacity(4 * 1024);
    let len = r.read_buf(&mut buf).await?;
    if len > 0 {
        let exchange = Command::Exchange(Exchange::new(context.get_conn_id(), buf.freeze()));
        context.command_sender.send(exchange).await?;
        Ok(())
    } else {
        let exchange = Command::Exchange(Exchange::new(context.get_conn_id(), Bytes::new()));
        context.command_sender.send(exchange).await?;
        Err(anyhow!("exchange local conn EOF"))
    }
}

async fn write_bytes(
    w: &mut WriteHalf<TcpStream>,
    rx: &mut mpsc::Receiver<Bytes>,
) -> anyhow::Result<()> {
    if let Some(bytes) = rx.recv().await {
        if bytes.is_empty() {
            Err(anyhow!("exchange remote conn close"))
        } else {
            w.write_all(&bytes).await?;
            Ok(())
        }
    } else {
        Err(anyhow!("exchange receiver none"))
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
