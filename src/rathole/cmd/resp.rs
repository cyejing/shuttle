use bytes::Bytes;

use crate::rathole::cmd::{CommandParse, CommandTo};
use crate::rathole::frame::{Frame, Parse, ParseError};

#[derive(Debug, Default)]
pub struct Resp {
    msg: String,
}

impl Resp {
    pub fn new(msg: String) -> Self {
        Resp { msg }
    }
}

impl CommandParse<Resp> for Resp {
    fn parse_frame(parse: &mut Parse) -> crate::Result<Resp> {
        match parse.next_string() {
            Ok(msg) => Ok(Resp::new(msg)),
            Err(ParseError::EndOfStream) => Ok(Resp::default()),
            Err(e) => Err(e.into()),
        }
    }
}

impl CommandTo for Resp {
    fn to_frame(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from("resp"));
        f.push_bulk(Bytes::from(self.msg.clone()));
        Ok(f)
    }
}
