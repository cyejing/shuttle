use std::time::Duration;

use anyhow::{Context, anyhow};
use bytes::{Bytes, BytesMut};
use tokio::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::CRLF;
use crate::config::HoleConfig;
use crate::rathole::cmd::Command;
use crate::rathole::cmd::exchange::Exchange;
use crate::rathole::cmd::hole::Hole;
use crate::rathole::dispatcher::Dispatcher;
use borer_core::tls::{make_server_name, make_tls_connector};

use self::cmd::ping::Ping;

pub mod cmd;
pub mod context;
pub mod dispatcher;
pub mod frame;

pub type ReqChannel = Option<oneshot::Sender<anyhow::Result<()>>>;
pub type CommandChannel = (u64, Command, ReqChannel);

pub async fn start_rathole(
    remote_addr: &str,
    invalid_certs: bool,
    hash: String,
    holes: &Vec<HoleConfig>,
) -> anyhow::Result<()> {
    let stream = timeout(Duration::from_secs(10), TcpStream::connect(remote_addr))
        .await
        .context("Connect remote timeout")?
        .context(format!("Can't connect remote addr {}", remote_addr))?;

    info!("Rathole connect remote {} success", remote_addr);
    let domain = make_server_name(remote_addr)?;
    let tls_stream = timeout(
        Duration::from_secs(10),
        make_tls_connector(invalid_certs).connect(domain, stream),
    )
    .await
    .context("Connect remote tls timeout")?
    .context("Connect remote tls failed")?;
    handle(tls_stream, hash, holes).await
}

async fn handle<T: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut stream: T,
    hash: String,
    holes: &Vec<HoleConfig>,
) -> anyhow::Result<()> {
    let mut buf: Vec<u8> = vec![];
    buf.extend_from_slice(hash.as_bytes());
    buf.extend_from_slice(&CRLF);

    timeout(Duration::from_secs(10), stream.write_all(buf.as_slice()))
        .await
        .context("Remote write shake hands timeout")?
        .context("Can't write rathole hash")?;

    let (mut dispatcher, command_sender) = Dispatcher::new(stream, hash);

    let (sx, rx) = oneshot::channel();
    tokio::spawn(async move { sx.send(dispatcher.dispatch().await) });

    let command_sender_cloned = command_sender.clone();
    tokio::spawn(async move {
        let mut last_ping = 60; // 10min
        loop {
            if last_ping > 0 {
                command_sender_cloned
                    .send_sync(Command::Ping(Ping::new(None)))
                    .await
                    .ok();
                last_ping -= 1;
            } else {
                break;
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    for hole in holes {
        let open_proxy =
            Command::Hole(Hole::new(hole.remote_addr.clone(), hole.local_addr.clone()));
        timeout(
            Duration::from_secs(10),
            command_sender.send_sync(open_proxy),
        )
        .await
        .context("Hole open proxy timeout")?
        .context("Hole open proxy failed")?;
        info!(
            "Hole open proxy for [remote:{}], [local:{}]",
            hole.remote_addr, hole.local_addr
        );
    }
    rx.await.context("Dispatcher stop")?
}

async fn exchange_copy(ts: TcpStream, mut rx: mpsc::Receiver<Bytes>, context: context::Context) {
    let (mut r, mut w) = io::split(ts);
    let mut shutdown = context.notify_shutdown.subscribe();
    info!(
        "start stream copy by exchange conn_id: {:?}",
        context.current_conn_id
    );
    tokio::select! {
        r1 = read_bytes(&mut r, context.clone()) => r1,
        r2 = write_bytes(&mut w, &mut rx) => r2,
        _ = shutdown.recv() =>{
            debug!("exchange recv shutdown signal");
            Ok(())
        }
    }
    .inspect_err(|e| debug!("exchange copy faield {e}"))
    .ok();

    info!(
        "stop stream copy by exchange conn_id: {:?}",
        context.current_conn_id
    );
}

async fn read_bytes(r: &mut ReadHalf<TcpStream>, context: context::Context) -> anyhow::Result<()> {
    loop {
        let mut buf = BytesMut::with_capacity(4 * 1024);
        let len = r
            .read_buf(&mut buf)
            .await
            .context("connection read byte err")?;
        if len > 0 {
            let exchange = Command::Exchange(Exchange::new(context.get_conn_id(), buf.freeze()));
            context.command_sender.send_sync(exchange).await?;
        } else {
            let exchange = Command::Exchange(Exchange::new(context.get_conn_id(), Bytes::new()));
            context.command_sender.send_sync(exchange).await?;
            return Err(anyhow!("exchange local conn EOF"));
        }
    }
}

async fn write_bytes(
    w: &mut WriteHalf<TcpStream>,
    rx: &mut mpsc::Receiver<Bytes>,
) -> anyhow::Result<()> {
    loop {
        if let Some(bytes) = rx.recv().await {
            trace!("recv exchange copy byte and write_all");
            if bytes.is_empty() {
                return Err(anyhow!("exchange remote conn close"));
            } else {
                w.write_all(&bytes)
                    .await
                    .context("connection write byte err")?;
            }
        } else {
            return Err(anyhow!("exchange receiver none"));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Cursor;

    use crate::rathole::cmd::Command;
    use crate::rathole::cmd::ping::Ping;
    use crate::rathole::cmd::resp::Resp;
    use crate::rathole::dispatcher::{CommandRead, CommandWrite};

    pub fn new_command_read(buf: &mut Vec<u8>) -> CommandRead<Cursor<Vec<u8>>> {
        let (r, _w) = tokio::io::split(Cursor::new(Vec::from(buf.as_slice())));
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
