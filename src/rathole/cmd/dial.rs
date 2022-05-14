use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::frame::{Frame, Parse};
use crate::store::ServerStore;
use bytes::Bytes;
use log::error;

#[derive(Debug)]
pub struct Dial {
    addr: String,
}

impl Dial {
    pub const COMMAND_NAME: &'static str = "dial";

    pub fn new(addr: String) -> Self {
        Dial { addr }
    }
}

impl CommandParse<Dial> for Dial {
    fn parse_frame(parse: &mut Parse) -> crate::Result<Dial> {
        let addr = parse.next_string()?;
        Ok(Self::new(addr))
    }
}

impl CommandTo for Dial {
    fn to_frame(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(Self::COMMAND_NAME));
        Ok(f)
    }
}

impl CommandApply for Dial {
    fn apply(&self, _store: ServerStore) -> crate::Result<Option<Resp>> {
        let dial_conn = DialConn::new(self.addr.clone());
        tokio::spawn(async move {
            if let Err(err) = dial_conn.start().await {
                error!("dial conn err : {:?}", err);
            }
        });
        Ok(None)
    }
}

#[allow(dead_code)]
pub struct DialConn {
    addr: String,
}

impl DialConn {
    pub fn new(addr: String) -> Self {
        DialConn { addr }
    }

    pub async fn start(&self) -> crate::Result<()> {
        Ok(())
    }
}
