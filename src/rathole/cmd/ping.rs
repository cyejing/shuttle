use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::frame::{Frame, Parse, ParseError};
use bytes::Bytes;

#[derive(Debug, Default)]
pub struct Ping {
    msg: Option<String>,
}

impl Ping {
    pub fn new(msg: Option<String>) -> Ping {
        Ping { msg }
    }
}

impl CommandParse<Ping> for Ping {
    fn parse_frame(parse: &mut Parse) -> crate::Result<Ping> {
        match parse.next_string() {
            Ok(msg) => Ok(Ping::new(Some(msg))),
            Err(ParseError::EndOfStream) => Ok(Ping::default()),
            Err(e) => Err(e.into()),
        }
    }
}

impl CommandApply for Ping {
    fn apply(&self) -> crate::Result<Option<Resp>> {
        let resp = match self.msg.clone() {
            None => Resp::new("PONG".to_string()),
            Some(msg) => Resp::new(msg),
        };
        Ok(Some(resp))
    }
}

impl CommandTo for Ping {
    fn to_frame(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from("ping"));
        if self.msg.is_some() {
            f.push_bulk(Bytes::from(self.msg.clone().unwrap()));
        }
        Ok(f)
    }
}
