use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::CommandApply;

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
    fn apply(&self) -> crate::Result<Option<Resp>> {
        let resp = Resp::new(format!("ERR unknown command '{}'", self.command_name));
        Ok(Some(resp))
    }
}
