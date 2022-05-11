use std::sync::Arc;
use async_trait::async_trait;
use crate::rathole::cmd::{Command, CommandApply, CommandExec, CommandParse};
use crate::rathole::cmd::resp::Resp;
use crate::rathole::connection::CmdSender;
use crate::rathole::parse::{Parse, ParseError};

#[derive(Debug)]
pub struct Unknown {
    command_name: String,
}

impl Unknown{
    pub fn new(command_name: String) -> Unknown {
        Unknown { command_name }
    }
}


#[async_trait]
impl CommandApply for Unknown{
    async fn apply(self, sender: Arc<CmdSender>) -> crate::Result<()> {
        let resp = Resp::Error(format!("ERR unknown command '{}'", self.command_name));
        sender.send(Command::Resp(resp)).await
    }
}
