use std::sync::Arc;

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use log::{error, info};
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::rathole::cmd::dial::Dial;
use crate::rathole::cmd::exchange::Exchange;
use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{Command, CommandApply, CommandParse, CommandTo};
use crate::rathole::dispatcher::{ConnSender, Context};
use crate::rathole::frame::{Frame, Parse};

#[derive(Debug)]
#[allow(dead_code)]
pub struct Proxy {
    remote_addr: String,
    local_addr: String,
}

impl Proxy {
    pub const COMMAND_NAME: &'static str = "proxy";

    pub fn new(remote_addr: String, local_addr: String) -> Self {
        Proxy {
            remote_addr,
            local_addr,
        }
    }
}

impl CommandParse<Proxy> for Proxy {
    fn parse_frame(parse: &mut Parse) -> crate::Result<Self> {
        let remote_addr = parse.next_string()?;
        let local_addr = parse.next_string()?;
        Ok(Proxy::new(remote_addr, local_addr))
    }
}

impl CommandTo for Proxy {
    fn to_frame(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(Self::COMMAND_NAME));
        f.push_bulk(Bytes::from(self.remote_addr.clone()));
        Ok(f)
    }
}

#[async_trait]
impl CommandApply for Proxy {
    async fn apply(&self, context: Context) -> crate::Result<Option<Resp>> {
        let proxy_server = ProxyServer::new(
            self.remote_addr.clone(),
            self.local_addr.clone(),
            context.clone(),
        );
        if let Err(err) = proxy_server.start().await {
            error!("dial conn err : {:?}", err);
        }
        Ok(None)
    }
}

#[allow(dead_code)]
pub struct ProxyServer {
    addr: String,
    local_addr: String,
    context: Context,
}

impl ProxyServer {
    pub fn new(addr: String, local_addr: String, context: Context) -> Self {
        ProxyServer {
            addr,
            local_addr,
            context,
        }
    }

    pub async fn start(self) -> crate::Result<()> {
        let listener = TcpListener::bind(self.addr.clone()).await?;
        tokio::spawn(async move {
            if let Err(e) = self.run(listener).await {
                error!("proxy run accept conn err: {}", e);
            }
        });
        Ok(())
    }

    async fn run(&self, listener: TcpListener) -> crate::Result<()> {
        loop {
            let (ts, _) = listener.accept().await?;
            info!("accept proxy conn");
            let (tx, rx) = mpsc::channel(24);
            let context = self.context.clone();

            let conn_id = Uuid::new_v4().to_string();
            let conn_sender = Arc::new(ConnSender::new(conn_id.clone(), tx));
            self.context.set_conn_sender(conn_sender).await;

            let dial = Command::Dial(Dial::new(self.local_addr.clone()));
            self.context.command_sender.send_sync(dial).await?;
            tokio::spawn(async move {
                if let Err(e) = Self::exchange(ts, rx, conn_id.clone(), context).await {
                    error!("exchange bytes err: {}", e);
                }
            });
        }
    }

    async fn exchange(
        ts: TcpStream,
        mut rx: mpsc::Receiver<Bytes>,
        conn_id: String,
        context: Context,
    ) -> crate::Result<()> {
        let (mut r, mut w) = io::split(ts);
        info!("select read and write bytes");
        loop {
            tokio::select! {
                r1 = Self::read_bytes(&mut r,context.clone(),conn_id.clone()) => r1?,
                r2 = Self::write_bytes(&mut w,&mut rx) => r2?,
            }
        }
    }

    async fn read_bytes(
        r: &mut ReadHalf<TcpStream>,
        context: Context,
        conn_id: String,
    ) -> crate::Result<()> {
        let mut buf = BytesMut::new();
        let len = r.read_buf(&mut buf).await?;
        if len > 0 {
            info!("read byte len:{}", len);
            let exchange = Command::Exchange(Exchange::new(conn_id, buf.freeze()));
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use log::{error, info};
    use tokio::sync::mpsc;

    use crate::logs::init_log;
    use crate::rathole::cmd::proxy::ProxyServer;
    use crate::rathole::dispatcher::{CommandSender, Context};

    // #[tokio::test]
    #[allow(dead_code)]
    async fn test_proxy() {
        init_log();
        let (sender, mut receiver) = mpsc::channel(128);
        let command_sender = Arc::new(CommandSender::new("hash".to_string(), sender));
        let context = Context::new(command_sender);
        let proxy_server = ProxyServer::new("127.0.0.1:6777".to_string(), "".to_string(), context);
        tokio::spawn(async move {
            if let Err(err) = proxy_server.start().await {
                error!("dial conn err : {:?}", err);
            }
        });
        loop {
            let cmd = receiver.recv().await;
            match cmd {
                Some((cmd, rc)) => {
                    info!("{:?}", cmd);
                    if let Some(s) = rc {
                        info!("send ok");
                        let _a = s.send(Ok(()));
                    }
                }
                None => panic!("channel close"),
            }
        }
    }
}
