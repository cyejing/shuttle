use std::fmt::Debug;

use tracing::{debug, error};

use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::context::Context;
use crate::rathole::frame::{Frame, Parse};
use bytes::Bytes;

pub struct Exchange {
    conn_id: u64,
    body: Bytes,
}

impl Exchange {
    pub const COMMAND_NAME: &'static str = "exchange";

    pub fn new(conn_id: u64, body: Bytes) -> Self {
        Exchange { conn_id, body }
    }
}

impl CommandParse<Exchange> for Exchange {
    fn parse_frame(parse: &mut Parse) -> anyhow::Result<Exchange> {
        let conn_id = parse.next_int()?;
        let body = parse.next_bytes()?;
        Ok(Exchange::new(conn_id, body))
    }
}

impl CommandTo for Exchange {
    fn to_frame(&self) -> anyhow::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(Self::COMMAND_NAME));
        f.push_int(self.conn_id);
        f.push_bulk(self.body.clone());
        Ok(f)
    }
}

impl CommandApply for Exchange {
    async fn apply(&self, mut context: Context) -> anyhow::Result<Option<Resp>> {
        let conn_id = self.conn_id;
        let bytes = self.body.clone();
        context.with_conn_id(conn_id);
        let op_sender = context.get_conn_sender().await;
        match op_sender {
            Some(sender) => {
                if let Err(e) = sender.send(bytes).await {
                    context.remove_conn_sender().await;
                    error!(error = %e, conn_id, "exchange send bytes failed");
                }
            }
            None => {
                debug!("exchange conn {} close", conn_id);
            }
        }
        Ok(Some(Resp::Ok("ok".to_string())))
    }
}

impl Debug for Exchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Exchange")
            .field("conn_id", &self.conn_id)
            .field("body", &self.body.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    #[test]
    fn test_exchange_new() {
        let exchange = Exchange::new(42, Bytes::from("test data"));
        assert_eq!(exchange.conn_id, 42);
        assert_eq!(exchange.body, Bytes::from("test data"));
    }

    #[test]
    fn test_exchange_command_name() {
        assert_eq!(Exchange::COMMAND_NAME, "exchange");
    }

    #[test]
    fn test_exchange_to_frame() {
        let exchange = Exchange::new(42, Bytes::from("test data"));
        let frame = exchange.to_frame().unwrap();

        match frame {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 3);
                assert!(vec[0] == "exchange");
                match &vec[1] {
                    Frame::Integer(val) => assert_eq!(*val, 42),
                    _ => panic!("expected integer"),
                }
                match &vec[2] {
                    Frame::Bulk(bytes) => assert_eq!(&bytes[..], b"test data"),
                    _ => panic!("expected bulk"),
                }
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_exchange_debug() {
        let exchange = Exchange::new(42, Bytes::from("test"));
        let debug_str = format!("{:?}", exchange);
        assert!(debug_str.contains("conn_id"));
        assert!(debug_str.contains("body"));
    }
}
