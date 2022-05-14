use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::frame::{Frame, Parse};
use crate::store::ServerStore;
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

impl CommandApply for Exchange {
    fn apply(&self, store: ServerStore) -> crate::Result<Option<Resp>> {
        let op_sender = store.get_conn_sender(&self.conn_id);
        match op_sender {
            Some(sender) => {
                let bytes = self.body.clone();
                tokio::spawn(async move {
                    if let Err(e) = sender.send(bytes).await {
                        error!("send bytes err : {}", e);
                    }
                });
                Ok(Some(Resp::Ok("".to_string())))
            }
            None => Ok(Some(Resp::Err("conn close".to_string()))),
        }
    }
}
