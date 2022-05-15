pub mod cmd;
pub mod dispatcher;
pub mod frame;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Cursor;

    use crate::rathole::cmd::ping::Ping;
    use crate::rathole::cmd::resp::Resp;
    use crate::rathole::cmd::Command;
    use crate::rathole::dispatcher::{CommandRead, CommandWrite};

    pub fn new_command_read(buf: &mut Vec<u8>) -> CommandRead<Cursor<Vec<u8>>> {
        let (r, _w) = tokio::io::split(Cursor::new(Vec::from(buf.as_slice().clone())));
        CommandRead::new(r)
    }

    pub fn new_command_write(buf: &mut Vec<u8>) -> CommandWrite<Cursor<&mut Vec<u8>>> {
        let (_r, w) = tokio::io::split(Cursor::new(buf));
        CommandWrite::new(w)
    }

    async fn command_write_read(cmd: Command) -> Command {
        let mut cell = RefCell::new(Vec::new());
        let mut command_write = new_command_write(cell.get_mut());
        command_write.write_command(10, cmd).await.unwrap();
        let mut command_read = new_command_read(cell.get_mut());
        let (req_id, cmd) = command_read.read_command().await.unwrap();
        assert_eq!(10, req_id);
        cmd
    }

    #[tokio::test]
    pub async fn test_command_ping() {
        let ping = Command::Ping(Ping::default());
        let cmd = command_write_read(ping).await;
        assert_eq!("Ping(Ping { msg: Some(\"pong\") })", format!("{:?}", cmd));
    }

    #[tokio::test]
    pub async fn test_command_resp() {
        let resp = Command::Resp(Resp::Ok("hi".to_string()));
        let cmd = command_write_read(resp).await;
        assert_eq!("Resp(Ok(\"hi\"))", format!("{:?}", cmd));
    }

    #[tokio::test]
    pub async fn test_command_resp_err() {
        let resp = Command::Resp(Resp::Err("hi".to_string()));
        let cmd = command_write_read(resp).await;
        assert_eq!("Resp(Err(\"hi\"))", format!("{:?}", cmd));
    }
}
