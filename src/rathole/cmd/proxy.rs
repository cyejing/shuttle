use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::rathole::{context, exchange_copy};
use crate::rathole::cmd::{Command, CommandApply, CommandParse, CommandTo};
use crate::rathole::cmd::dial::Dial;
use crate::rathole::cmd::resp::Resp;
use crate::rathole::context::{ConnSender, IdAdder};
use crate::rathole::frame::{Frame, Parse};

#[derive(Debug)]
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
    fn parse_frame(parse: &mut Parse) -> anyhow::Result<Self> {
        let remote_addr = parse.next_string()?;
        let local_addr = parse.next_string()?;
        Ok(Proxy::new(remote_addr, local_addr))
    }
}

impl CommandTo for Proxy {
    fn to_frame(&self) -> anyhow::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(Self::COMMAND_NAME));
        f.push_bulk(Bytes::from(self.remote_addr.clone()));
        f.push_bulk(Bytes::from(self.local_addr.clone()));
        Ok(f)
    }
}

#[async_trait]
impl CommandApply for Proxy {
    async fn apply(&self, context: context::Context) -> anyhow::Result<Option<Resp>> {
        let proxy_server = ProxyServer::new(
            self.remote_addr.clone(),
            self.local_addr.clone(),
            context.clone(),
        );
        match proxy_server.start().await {
            Ok(_) => Ok(Some(Resp::Ok("ok".to_string()))),
            Err(e) => {
                error!("dial conn err : {:?}", e);
                Ok(Some(Resp::Err(format!("{}", e))))
            }
        }
    }
}

pub struct ProxyServer {
    addr: String,
    local_addr: String,
    context: context::Context,
    id_adder: IdAdder,
}

impl ProxyServer {
    pub fn new(addr: String, local_addr: String, context: context::Context) -> Self {
        ProxyServer {
            addr,
            local_addr,
            context,
            id_adder: IdAdder::default(),
        }
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.addr).await?;
        info!("Bind proxy server {}", &self.addr);
        let mut shutdown = self.context.notify_shutdown.subscribe();
        tokio::spawn(async move {
            tokio::select! {
                r1 = self.run(listener) => {
                    if let Err(e) = r1 {
                        error!("proxy run accept conn err: {}", e);
                    }
                },
                _ = shutdown.recv() =>{
                    info!("recv shutdown signal")
                }
            }
        });
        Ok(())
    }

    async fn run(&mut self, listener: TcpListener) -> anyhow::Result<()> {
        loop {
            let (ts, _) = listener.accept().await?;
            info!("accept proxy conn");
            let (tx, rx) = mpsc::channel(24);

            let conn_id = self.id_adder.add_and_get().await;
            let conn_sender = Arc::new(ConnSender::new(conn_id, tx));
            self.context.set_conn_sender(conn_sender).await;

            let mut context = self.context.clone();
            context.with_conn_id(conn_id);

            let dial = Command::Dial(Dial::new(conn_id, self.local_addr.clone()));
            context.command_sender.send_sync(dial).await?;
            tokio::spawn(async move {
                if let Err(e) = exchange_copy(ts, rx, context.clone()).await {
                    error!("exchange bytes err: {}", e);
                }
                context.remove_conn_sender().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {}
