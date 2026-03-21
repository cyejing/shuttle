use crate::rathole::cmd::dial::Dial;
use crate::rathole::cmd::exchange::Exchange;
use crate::rathole::cmd::hole::Hole;
use crate::rathole::cmd::ping::Ping;
use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::unknown::Unknown;
use crate::rathole::context::Context;
use crate::rathole::frame::{Frame, Parse};
use anyhow::anyhow;
use std::fmt::Debug;

pub mod dial;
pub mod exchange;
pub mod hole;
pub mod ping;
pub mod resp;
pub mod unknown;

#[derive(Debug)]
pub enum Command {
    Dial(Dial),
    Exchange(Exchange),
    Ping(Ping),
    Hole(Hole),
    Resp(Resp),
    Unknown(Unknown),
}

impl Command {
    pub fn from_frame(frame: Frame) -> anyhow::Result<(u64, Command)> {
        let mut parse = Parse::new(frame)?;
        let command_name = parse.next_string()?.to_lowercase();
        let command = match &command_name[..] {
            Dial::COMMAND_NAME => Command::Dial(Dial::parse_frame(&mut parse)?),
            Exchange::COMMAND_NAME => Command::Exchange(Exchange::parse_frame(&mut parse)?),
            Ping::COMMAND_NAME => Command::Ping(Ping::parse_frame(&mut parse)?),
            Hole::COMMAND_NAME => Command::Hole(Hole::parse_frame(&mut parse)?),
            Resp::COMMAND_NAME => Command::Resp(Resp::parse_frame(&mut parse)?),
            _ => return Ok((0, Command::Unknown(Unknown::new(command_name)))),
        };

        let req_id = parse.next_int()?;

        parse.finish()?;

        Ok((req_id, command))
    }

    pub async fn apply(self, context: Context) -> anyhow::Result<Option<Command>> {
        use Command::*;

        trace!("apply command {:?}", &self);
        let resp = match self {
            Dial(dial) => dial.apply(context).await?,
            Exchange(exchange) => exchange.apply(context).await?,
            Ping(ping) => ping.apply(context).await?,
            Hole(hole) => hole.apply(context).await?,
            Resp(resp) => resp.apply(context).await?,
            Unknown(unknown) => unknown.apply(context).await?,
        };
        let oc = resp.map(Command::Resp);
        Ok(oc)
    }

    pub fn to_frame(self, req_id: u64) -> anyhow::Result<Frame> {
        use Command::*;

        let f = match self {
            Dial(dial) => dial.to_frame()?.push_req_id(req_id),
            Exchange(exchange) => exchange.to_frame()?.push_req_id(req_id),
            Ping(ping) => ping.to_frame()?.push_req_id(req_id),
            Hole(hole) => hole.to_frame()?.push_req_id(req_id),
            Resp(resp) => resp.to_frame()?.push_req_id(req_id),
            _ => return Err(anyhow!("command undo")),
        };
        Ok(f)
    }
}

pub trait CommandParse<T> {
    fn parse_frame(parse: &mut Parse) -> anyhow::Result<T>;
}

pub(crate) trait CommandApply {
    async fn apply(&self, context: Context) -> anyhow::Result<Option<Resp>>;
}

pub trait CommandTo {
    fn to_frame(&self) -> anyhow::Result<Frame>;
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    #[test]
    fn test_command_from_frame_dial() {
        let frame = Frame::Array(vec![
            Frame::Bulk(Bytes::from("dial")),
            Frame::Integer(1),
            Frame::Bulk(Bytes::from("127.0.0.1:8080")),
            Frame::Integer(10),
        ]);

        let (req_id, cmd) = Command::from_frame(frame).unwrap();
        assert_eq!(req_id, 10);
        match cmd {
            Command::Dial(_) => {}
            _ => panic!("expected dial command"),
        }
    }

    #[test]
    fn test_command_from_frame_exchange() {
        let frame = Frame::Array(vec![
            Frame::Bulk(Bytes::from("exchange")),
            Frame::Integer(42),
            Frame::Bulk(Bytes::from("test data")),
            Frame::Integer(5),
        ]);

        let (req_id, cmd) = Command::from_frame(frame).unwrap();
        assert_eq!(req_id, 5);
        match cmd {
            Command::Exchange(_) => {}
            _ => panic!("expected exchange command"),
        }
    }

    #[test]
    fn test_command_from_frame_ping() {
        let frame = Frame::Array(vec![
            Frame::Bulk(Bytes::from("ping")),
            Frame::Bulk(Bytes::from("pong")),
            Frame::Integer(1),
        ]);

        let (req_id, cmd) = Command::from_frame(frame).unwrap();
        assert_eq!(req_id, 1);
        match cmd {
            Command::Ping(_) => {}
            _ => panic!("expected ping command"),
        }
    }

    #[test]
    fn test_command_from_frame_hole() {
        let frame = Frame::Array(vec![
            Frame::Bulk(Bytes::from("hole")),
            Frame::Bulk(Bytes::from("127.0.0.1:8080")),
            Frame::Bulk(Bytes::from("127.0.0.1:9090")),
            Frame::Integer(1),
        ]);

        let (req_id, cmd) = Command::from_frame(frame).unwrap();
        assert_eq!(req_id, 1);
        match cmd {
            Command::Hole(_) => {}
            _ => panic!("expected hole command"),
        }
    }

    #[test]
    fn test_command_from_frame_resp_ok() {
        let frame = Frame::Array(vec![
            Frame::Bulk(Bytes::from("resp")),
            Frame::Simple("ok".to_string()),
            Frame::Integer(1),
        ]);

        let (req_id, cmd) = Command::from_frame(frame).unwrap();
        assert_eq!(req_id, 1);
        match cmd {
            Command::Resp(resp) => match resp {
                Resp::Ok(msg) => assert_eq!(msg, "ok"),
                _ => panic!("expected ok resp"),
            },
            _ => panic!("expected resp command"),
        }
    }

    #[test]
    fn test_command_from_frame_resp_err() {
        let frame = Frame::Array(vec![
            Frame::Bulk(Bytes::from("resp")),
            Frame::Error("error message".to_string()),
            Frame::Integer(1),
        ]);

        let (req_id, cmd) = Command::from_frame(frame).unwrap();
        assert_eq!(req_id, 1);
        match cmd {
            Command::Resp(resp) => match resp {
                Resp::Err(msg) => assert_eq!(msg, "error message"),
                _ => panic!("expected error resp"),
            },
            _ => panic!("expected resp command"),
        }
    }

    #[test]
    fn test_command_from_frame_unknown() {
        let frame = Frame::Array(vec![
            Frame::Bulk(Bytes::from("unknown_command")),
            Frame::Integer(0),
        ]);

        let (req_id, cmd) = Command::from_frame(frame).unwrap();
        assert_eq!(req_id, 0);
        match cmd {
            Command::Unknown(_) => {}
            _ => panic!("expected unknown command"),
        }
    }

    #[test]
    fn test_command_to_frame_dial() {
        let dial = Dial::new(1, "127.0.0.1:8080".to_string());
        let frame = Command::Dial(dial).to_frame(10).unwrap();

        match frame {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 4);
                assert!(vec[0] == "dial");
                match &vec[3] {
                    Frame::Integer(val) => assert_eq!(*val, 10),
                    _ => panic!("expected integer"),
                }
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_command_to_frame_exchange() {
        let exchange = Exchange::new(42, Bytes::from("data"));
        let frame = Command::Exchange(exchange).to_frame(5).unwrap();

        match frame {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 4);
                assert!(vec[0] == "exchange");
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_command_to_frame_ping() {
        let ping = Ping::new(Some("test".to_string()));
        let frame = Command::Ping(ping).to_frame(1).unwrap();

        match frame {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 3);
                assert!(vec[0] == "ping");
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_command_to_frame_hole() {
        let hole = Hole::new("127.0.0.1:8080".to_string(), "127.0.0.1:9090".to_string());
        let frame = Command::Hole(hole).to_frame(1).unwrap();

        match frame {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 4);
                assert!(vec[0] == "hole");
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_command_to_frame_resp() {
        let resp = Resp::Ok("success".to_string());
        let frame = Command::Resp(resp).to_frame(1).unwrap();

        match frame {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 3);
                assert!(vec[0] == "resp");
            }
            _ => panic!("expected array"),
        }
    }
}
