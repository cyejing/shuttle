use std::sync::Arc;

use async_trait::async_trait;

use crate::rathole::cmd::ping::Ping;
use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::unknown::Unknown;
use crate::rathole::session::{CommandSender};
use crate::rathole::frame::{Frame, Parse};

pub mod ping;
pub mod dial;
pub mod exchange;
pub mod proxy;
pub mod resp;
pub mod unknown;

#[derive(Debug)]
pub enum Command {
    Dial,
    Exchange,
    Ping(Ping),
    Proxy,
    Resp(Resp),
    Unknown(Unknown),
}

impl Command {
    pub fn from_frame(frame: Frame) -> crate::Result<Command> {
        let mut parse = Parse::new(frame)?;
        let command_name = parse.next_string()?.to_lowercase();
        let command = match &command_name[..] {
            "ping" => Command::Ping(Ping::parse_frames(&mut parse)?),
            "resp" => Command::Resp(Resp::parse_frames(&mut parse)?),
            _ => return Ok(Command::Unknown(Unknown::new(command_name))),
        };

        parse.finish()?;

        Ok(command)
    }

    pub async fn apply(self, sender: Arc<CommandSender>) -> crate::Result<()> {
        use Command::*;

        match self {
            Ping(ping) => ping.apply(sender).await,
            Resp(resp) => resp.apply(sender).await,
            Unknown(unknown) => unknown.apply(sender).await,
            _ => Err("undo".into()),
        }
    }

    pub fn exec(self) -> crate::Result<Frame> {
        use Command::*;

        match self {
            Resp(resp) => resp.exec(),
            Ping(ping) => ping.exec(),
            _ => Err("undo".into()),
        }
    }
}

pub trait CommandParse<T> {
    fn parse_frames(parse: &mut Parse) -> crate::Result<T>;
}

#[async_trait]
pub trait CommandApply {
    async fn apply(&self, sender: Arc<CommandSender>) -> crate::Result<()>;
}

pub trait CommandExec {
    fn exec(&self) -> crate::Result<Frame>;
}