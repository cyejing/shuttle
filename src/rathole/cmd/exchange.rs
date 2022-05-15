use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::dispatcher::Context;
use crate::rathole::frame::{Frame, Parse};
use async_trait::async_trait;
use bytes::Bytes;
use log::error;

#[derive(Debug)]
pub struct Exchange {
    conn_id: String,
    body: Bytes,
}

impl Exchange {
    pub const COMMAND_NAME: &'static str = "exchange";

    pub fn new(conn_id: String, body: Bytes) -> Self {
        Exchange { conn_id, body }
    }
}

impl CommandParse<Exchange> for Exchange {
    fn parse_frame(parse: &mut Parse) -> crate::Result<Exchange> {
        let conn_id = parse.next_string()?;
        let body = parse.next_bytes()?;
        Ok(Exchange::new(conn_id, body))
    }
}

impl CommandTo for Exchange {
    fn to_frame(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(self.conn_id.clone()));
        f.push_bulk(self.body.clone());
        Ok(f)
    }
}

#[async_trait]
impl CommandApply for Exchange {
    async fn apply(&self, context: Context) -> crate::Result<Option<Resp>> {
        let conn_id = self.conn_id.clone();
        let bytes = self.body.clone();
        let op_sender = context.get_conn_sender(&conn_id).await;
        match op_sender {
            Some(sender) => {
                if let Err(e) = sender.send(bytes).await {
                    error!("send bytes err : {}", e);
                }
            }
            None => {
                error!("exchange conn close");
            }
        }
        Ok(None)
    }
}
