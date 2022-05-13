use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use crate::rathole::cmd::{Command, CommandApply, CommandExec, CommandParse};
use crate::rathole::cmd::resp::Resp;
use crate::rathole::session::CommandSender;
use crate::rathole::frame::{Frame, Parse, ParseError};

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
    fn parse_frames(parse: &mut Parse) -> crate::Result<Ping> {
        match parse.next_string() {
            Ok(msg) => Ok(Ping::new(Some(msg))),
            Err(ParseError::EndOfStream) => Ok(Ping::default()),
            Err(e) => Err(e.into()),
        }
    }
}

#[async_trait]
impl CommandApply for Ping {
    async fn apply(&self, sender: Arc<CommandSender>) -> crate::Result<()> {
        let resp = match self.msg.clone() {
            None => Resp::new("PONG".to_string()),
            Some(msg) => Resp::new(msg),
        };

        sender.send(Command::Resp(resp)).await
    }
}

impl CommandExec for Ping {
    fn exec(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from("ping"));
        if self.msg.is_some(){
            f.push_bulk(Bytes::from(self.msg.clone().unwrap()));
        }
        Ok(f)
    }
}