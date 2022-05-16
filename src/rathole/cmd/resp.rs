use async_trait::async_trait;
use bytes::Bytes;
use log::{debug, error};

use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::context::Context;
use crate::rathole::frame::{Frame, Parse};

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

#[async_trait]
impl CommandApply for Resp {
    async fn apply(&self, context: Context) -> crate::Result<Option<Resp>> {
        let rc = context.get_req().await;
        if let Some(s) = rc.flatten() {
            if match self {
                Resp::Ok(_msg) => s.send(Ok(())),
                Resp::Err(msg) => s.send(Err(msg.to_string().into())),
            }
            .is_err()
            {
                error!("req channel close");
            }
        };
        debug!("resp : {:?}", self);
        Ok(None)
    }
}
