use bytes::Bytes;
use crate::rathole::connection::CmdSender;
use crate::rathole::frame::Frame;
use crate::rathole::parse::{Parse, ParseError};

#[derive(Debug, Default)]
pub struct Ping {
    msg: Option<String>,
}

impl Ping {
    pub fn new(msg: Option<String>) -> Ping {
        Ping { msg }
    }

    pub(crate) fn parse_frames(parse: &mut Parse) -> crate::Result<Ping> {
        match parse.next_string() {
            Ok(msg) => Ok(Ping::new(Some(msg))),
            Err(ParseError::EndOfStream) => Ok(Ping::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub(crate) async fn apply(self, dst: &mut CmdSender) -> crate::Result<()> {
        let _response = match self.msg {
            None => Frame::Simple("PONG".to_string()),
            Some(msg) => Frame::Bulk(Bytes::from(msg)),
        };
        //
        // // Write the response back to the client
        // dst.write_frame(&response).await?;

        Ok(())
    }
}
