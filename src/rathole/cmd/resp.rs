use bytes::Bytes;

use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::frame::{Frame, Parse};
use crate::store::ServerStore;

#[derive(Debug)]
pub enum Resp {
    Ok(String),
    Err(String),
}

impl Resp {
    pub const COMMAND_NAME: &'static str = "resp";
}

impl CommandParse<Resp> for Resp {
    fn parse_frame(parse: &mut Parse) -> crate::Result<Resp> {
        match parse.next_part()? {
            Frame::Simple(msg) => Ok(Resp::Ok(msg)),
            Frame::Error(msg) => Ok(Resp::Err(msg)),
            frame => Err(format!(
                "protocol error; expected simple frame or err frame, got {:?}",
                frame
            )
            .into()),
        }
    }
}

impl CommandTo for Resp {
    fn to_frame(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(Resp::COMMAND_NAME));
        match self {
            Self::Ok(msg) => f.push_frame(Frame::Simple(msg.clone())),
            Self::Err(msg) => f.push_frame(Frame::Error(msg.clone())),
        };
        Ok(f)
    }
}

impl CommandApply for Resp {
    fn apply(&self, _store: ServerStore) -> crate::Result<Option<Resp>> {
        Ok(None)
    }
}
