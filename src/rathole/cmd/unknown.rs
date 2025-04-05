use crate::rathole::cmd::CommandApply;
use crate::rathole::cmd::resp::Resp;
use crate::rathole::context::Context;

#[derive(Debug)]
pub struct Unknown {
    command_name: String,
}

impl Unknown {
    pub fn new(command_name: String) -> Unknown {
        Unknown { command_name }
    }
}

impl CommandApply for Unknown {
    async fn apply(&self, _context: Context) -> anyhow::Result<Option<Resp>> {
        let resp = Resp::Err(format!("ERR unknown command '{}'", self.command_name));
        Ok(Some(resp))
    }
}
