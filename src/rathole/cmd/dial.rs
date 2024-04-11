use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::context::{ConnSender, Context};
use crate::rathole::exchange_copy;
use crate::rathole::frame::{Frame, Parse};
use bytes::Bytes;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct Dial {
    conn_id: u64,
    addr: String,
}

impl Dial {
    pub const COMMAND_NAME: &'static str = "dial";

    pub fn new(conn_id: u64, addr: String) -> Self {
        Dial { conn_id, addr }
    }
}

impl CommandParse<Dial> for Dial {
    fn parse_frame(parse: &mut Parse) -> anyhow::Result<Dial> {
        let conn_id = parse.next_int()?;
        let addr = parse.next_string()?;
        Ok(Self::new(conn_id, addr))
    }
}

impl CommandTo for Dial {
    fn to_frame(&self) -> anyhow::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(Self::COMMAND_NAME));
        f.push_int(self.conn_id);
        f.push_bulk(Bytes::from(self.addr.clone()));
        Ok(f)
    }
}

impl CommandApply for Dial {
    async fn apply(&self, context: Context) -> anyhow::Result<Option<Resp>> {
        let dial_conn = DialConn::new(self.conn_id, self.addr.clone());
        if let Err(err) = dial_conn.start(context).await {
            error!("dial conn err : {:?}", err);
        }
        Ok(Some(Resp::Ok("ok".to_string())))
    }
}

pub struct DialConn {
    conn_id: u64,
    addr: String,
}

impl DialConn {
    pub fn new(conn_id: u64, addr: String) -> Self {
        DialConn { addr, conn_id }
    }

    pub async fn start(&self, mut context: Context) -> anyhow::Result<()> {
        let stream = TcpStream::connect(&self.addr).await?;
        let conn_id = self.conn_id;

        tokio::spawn(async move {
            let (tx, rx) = mpsc::channel(1);
            let conn_sender = ConnSender::new(conn_id, tx);
            context.with_conn_id(conn_id);
            context.set_conn_sender(conn_sender).await;
            exchange_copy(stream, rx, context.clone()).await;
            context.remove_conn_sender().await;
        });
        Ok(())
    }
}
