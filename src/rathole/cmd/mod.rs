use crate::rathole::cmd::Command::Unknown;
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
    Ping,
    Proxy,
    Resp,
    Unknown,
}

impl Command {

    pub fn from_frame(frame: Frame) -> crate::Result<Command> {
        let mut parse = Parse::new(frame)?;
        let command_name = parse.next_string()?.to_lowercase();
        let _command = match &command_name[..] {
            "ping" => Command::Ping,
            _ => Command::Unknown,
        };

        Ok(Unknown)
    }
}
