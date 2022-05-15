use crate::rathole::cmd::dial::Dial;
use crate::rathole::cmd::exchange::Exchange;
use crate::rathole::cmd::ping::Ping;
use crate::rathole::cmd::proxy::Proxy;
use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::unknown::Unknown;
use crate::rathole::dispatcher::Context;
use crate::rathole::frame::{Frame, Parse};
use async_trait::async_trait;
use std::fmt::Debug;

pub mod dial;
pub mod exchange;
pub mod ping;
pub mod proxy;
pub mod resp;
pub mod unknown;

#[derive(Debug)]
pub enum Command {
    Dial(Dial),
    Exchange(Exchange),
    Ping(Ping),
    Proxy(Proxy),
    Resp(Resp),
    RespId(u64, Resp),
    Unknown(Unknown),
}

impl Command {
    pub fn from_frame(frame: Frame) -> crate::Result<(u64, Command)> {
        let mut parse = Parse::new(frame)?;
        let command_name = parse.next_string()?.to_lowercase();
        let command = match &command_name[..] {
            Dial::COMMAND_NAME => Command::Dial(Dial::parse_frame(&mut parse)?),
            Exchange::COMMAND_NAME => Command::Exchange(Exchange::parse_frame(&mut parse)?),
            Ping::COMMAND_NAME => Command::Ping(Ping::parse_frame(&mut parse)?),
            Proxy::COMMAND_NAME => Command::Proxy(Proxy::parse_frame(&mut parse)?),
            Resp::COMMAND_NAME => Command::Resp(Resp::parse_frame(&mut parse)?),
            _ => return Ok((0, Command::Unknown(Unknown::new(command_name)))),
        };

        let req_id = parse.next_int()?;

        parse.finish()?;

        Ok((req_id, command))
    }

    pub async fn apply(self, context: Context) -> crate::Result<Option<Command>> {
        use Command::*;

        let resp = match self {
            Dial(dial) => dial.apply(context).await?,
            Exchange(exchange) => exchange.apply(context).await?,
            Ping(ping) => ping.apply(context).await?,
            Proxy(proxy) => proxy.apply(context).await?,
            Resp(resp) => resp.apply(context).await?,
            RespId(_, resp) => resp.apply(context).await?,
            Unknown(unknown) => unknown.apply(context).await?,
        };
        let oc = resp.map(Command::Resp);
        Ok(oc)
    }

    pub fn to_frame(self, req_id: u64) -> crate::Result<Frame> {
        use Command::*;

        let f = match self {
            Dial(dial) => dial.to_frame()?.push_req_id(req_id),
            Exchange(exchange) => exchange.to_frame()?.push_req_id(req_id),
            Ping(ping) => ping.to_frame()?.push_req_id(req_id),
            Proxy(proxy) => proxy.to_frame()?.push_req_id(req_id),
            Resp(resp) => resp.to_frame()?.push_req_id(req_id),
            RespId(oid, resp) => resp.to_frame()?.push_req_id(oid),
            _ => return Err("undo".into()),
        };
        Ok(f)
    }
}

pub trait CommandParse<T> {
    fn parse_frame(parse: &mut Parse) -> crate::Result<T>;
}

#[async_trait]
pub trait CommandApply {
    async fn apply(&self, context: Context) -> crate::Result<Option<Resp>>;
}

pub trait CommandTo {
    fn to_frame(&self) -> crate::Result<Frame>;
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    #[test]
    fn test_uuid() {
        let uuid = Uuid::new_v4();
        println!("{}", uuid.to_string());
    }
}
