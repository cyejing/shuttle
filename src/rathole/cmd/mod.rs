use std::fmt::Debug;

use crate::rathole::cmd::ping::Ping;
use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::unknown::Unknown;
use crate::rathole::frame::{Frame, Parse};

pub mod dial;
pub mod exchange;
pub mod ping;
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
            "ping" => Command::Ping(Ping::parse_frame(&mut parse)?),
            "resp" => Command::Resp(Resp::parse_frame(&mut parse)?),
            _ => return Ok(Command::Unknown(Unknown::new(command_name))),
        };

        parse.finish()?;

        Ok(command)
    }

    pub fn apply(self) -> crate::Result<Option<Command>> {
        use Command::*;

        let resp = match self {
            Ping(ping) => ping.apply()?,
            Unknown(unknown) => unknown.apply()?,
            _ => None,
        };
        let oc = resp.map(Command::Resp);
        Ok(oc)
    }

    pub fn to_frame(self) -> crate::Result<Frame> {
        use Command::*;

        match self {
            Resp(resp) => resp.to_frame(),
            Ping(ping) => ping.to_frame(),
            _ => Err("undo".into()),
        }
    }
}

pub trait CommandParse<T> {
    fn parse_frame(parse: &mut Parse) -> crate::Result<T>;
}

pub trait CommandApply {
    fn apply(&self) -> crate::Result<Option<Resp>>;
}

pub trait CommandTo {
    fn to_frame(&self) -> crate::Result<Frame>;
}

#[cfg(test)]
mod tests {
    use crate::rathole::cmd;
    use crate::rathole::cmd::Command;

    #[test]
    fn test_option() {
        let none: Option<cmd::Resp> = None;
        let resp = none.map(Command::Resp);
        println!("{:?}", resp);

        let resp = Some(cmd::Resp::new("hi".to_string()));
        let cresp = resp.map(Command::Resp);
        println!("{:?}", cresp);
    }
}
