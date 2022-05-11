use std::sync::Arc;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};
use crate::rathole::cmd::{CommandApply, CommandExec, CommandParse};
use crate::rathole::connection::{CmdSender, Connection};
use crate::rathole::parse::Parse;

#[derive(Debug)]
pub enum Resp {
    Simple(String),
    Error(String),
    Integer(u64),
    Null,
}


#[async_trait]
impl CommandApply for Resp{
    async fn apply(self, sender: Arc<CmdSender>) -> crate::Result<()> {
        todo!()
    }
}

impl CommandParse<Resp> for Resp{
    fn parse_frames(parse: &mut Parse) -> crate::Result<Resp> {
        todo!()
    }
}

#[async_trait]
impl<T: AsyncRead + AsyncWrite + Unpin + Send>  CommandExec<T> for Resp{
    async fn exec(self, conn: &mut Connection<T>) -> crate::Result<()> {
        // conn.write_frame();
        todo!()
    }
}