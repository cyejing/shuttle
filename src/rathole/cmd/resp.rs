use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use log::debug;

use crate::rathole::cmd::{CommandApply, CommandExec, CommandParse};
use crate::rathole::frame::{Frame, Parse, ParseError};
use crate::rathole::session::CommandSender;

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
    fn parse_frames(parse: &mut Parse) -> crate::Result<Resp> {
        match parse.next_string() {
            Ok(msg) => Ok(Resp::new(msg)),
            Err(ParseError::EndOfStream) => Ok(Resp::default()),
            Err(e) => Err(e.into()),
        }
    }
}

#[async_trait]
impl CommandApply for Resp {
    async fn apply(&self, _sender: Arc<CommandSender>) -> crate::Result<()> {
        debug!("resp : {:?}", self);
        todo!()
    }
}

impl CommandExec for Resp {
    fn exec(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from("resp"));
        f.push_bulk(Bytes::from(self.msg.clone()));
        Ok(f)
    }
}