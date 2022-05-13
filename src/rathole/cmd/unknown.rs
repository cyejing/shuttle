use std::sync::Arc;
use async_trait::async_trait;
use crate::rathole::cmd::{Command, CommandApply};
use crate::rathole::cmd::resp::Resp;
use crate::rathole::session::CommandSender;

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
    async fn apply(&self, sender: Arc<CommandSender>) -> crate::Result<()> {
        let resp = Resp::new(format!("ERR unknown command '{}'", self.command_name));
        sender.send(Command::Resp(resp)).await
    }
}
