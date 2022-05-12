use std::sync::Arc;

use async_trait::async_trait;
use crate::rathole::cmd::{Command, CommandApply, CommandParse};
use crate::rathole::cmd::resp::Resp;
use crate::rathole::session::CmdSender;
use crate::rathole::parse::{Parse, ParseError};

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
    async fn apply(self, sender: Arc<CmdSender>) -> crate::Result<()> {
        let resp = match self.msg {
            None => Resp::new("PONG".to_string()),
            Some(msg) => Resp::new(msg),
        };

        sender.send(Command::Resp(resp)).await
    }
}
