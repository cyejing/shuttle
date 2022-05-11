use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::rathole::cmd::ping::Ping;
use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::unknown::Unknown;
use crate::rathole::connection::{CmdSender, Connection};
use crate::rathole::frame::Frame;
use crate::rathole::parse::Parse;
use crate::rathole::shutdown;

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

    pub async fn apply(self, sender: Arc<CmdSender>) -> crate::Result<()> {
        use Command::*;

        match self {
            Ping(ping) => ping.apply(sender).await,
            Resp(resp) => resp.apply(sender).await,
            Unknown(unknown) => unknown.apply(sender).await,
            _ => Err("undo".into()),
        }
    }

    pub async fn exec<T: AsyncRead + AsyncWrite + Unpin + Send>(self, conn: &mut Connection<T>) -> crate::Result<()> {
        use Command::*;

        match self {
            Resp(resp) => resp.exec(conn).await,
            _ => Err("undo".into()),
        }
    }
}

pub trait CommandParse<T> {
    fn parse_frames(parse: &mut Parse) -> crate::Result<T>;
}

#[async_trait]
pub trait CommandApply {
    async fn apply(self, sender: Arc<CmdSender>) -> crate::Result<()>;
}

#[async_trait]
pub trait CommandExec<T: AsyncRead + AsyncWrite + Unpin + Send> {
    async fn exec(self, conn: &mut Connection<T>) -> crate::Result<()>;
}