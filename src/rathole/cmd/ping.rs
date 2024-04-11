use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::context::Context;
use crate::rathole::frame::{Frame, Parse, ParseError};
use bytes::Bytes;

#[derive(Debug, Default)]
pub struct Ping {
    msg: Option<String>,
}

impl Ping {
    pub const COMMAND_NAME: &'static str = "ping";

    pub fn new(msg: Option<String>) -> Ping {
        Ping { msg }
    }
}

impl CommandParse<Ping> for Ping {
    fn parse_frame(parse: &mut Parse) -> anyhow::Result<Ping> {
        match parse.next_string() {
            Ok(msg) => Ok(Ping::new(Some(msg))),
            Err(ParseError::EndOfStream) => Ok(Ping::default()),
            Err(e) => Err(e.into()),
        }
    }
}

impl CommandTo for Ping {
    fn to_frame(&self) -> anyhow::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(Ping::COMMAND_NAME));
        let msg = self.msg.as_ref().map_or("pong".to_string(), |x| x.clone());
        f.push_bulk(Bytes::from(msg));
        Ok(f)
    }
}

impl CommandApply for Ping {
    async fn apply(&self, _context: Context) -> anyhow::Result<Option<Resp>> {
        let resp = match &self.msg {
            None => Resp::Ok("PONG".to_string()),
            Some(msg) => Resp::Ok(msg.clone()),
        };
        Ok(Some(resp))
    }
}
