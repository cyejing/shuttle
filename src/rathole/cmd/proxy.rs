use crate::rathole::cmd::resp::Resp;
use crate::rathole::cmd::{CommandApply, CommandParse, CommandTo};
use crate::rathole::frame::{Frame, Parse};
use crate::store::ServerStore;
use bytes::Bytes;
use log::error;

#[derive(Debug)]
#[allow(dead_code)]
pub struct Proxy {
    remote_addr: String,
    local_addr: String,
}

impl Proxy {
    pub const COMMAND_NAME: &'static str = "proxy";

    pub fn new(remote_addr: String, local_addr: String) -> Self {
        Proxy {
            remote_addr,
            local_addr,
        }
    }
}

impl CommandParse<Proxy> for Proxy {
    fn parse_frame(parse: &mut Parse) -> crate::Result<Self> {
        let remote_addr = parse.next_string()?;
        let local_addr = parse.next_string()?;
        Ok(Proxy::new(remote_addr, local_addr))
    }
}

impl CommandTo for Proxy {
    fn to_frame(&self) -> crate::Result<Frame> {
        let mut f = Frame::array();
        f.push_bulk(Bytes::from(Self::COMMAND_NAME));
        f.push_bulk(Bytes::from(self.remote_addr.clone()));
        Ok(f)
    }
}

impl CommandApply for Proxy {
    fn apply(&self, _store: ServerStore) -> crate::Result<Option<Resp>> {
        let proxy_server = ProxyServer::new(self.remote_addr.clone());
        tokio::spawn(async move {
            if let Err(err) = proxy_server.start().await {
                error!("dial conn err : {:?}", err);
            }
        });

        Ok(None)
    }
}

#[allow(dead_code)]
pub struct ProxyServer {
    addr: String,
}

impl ProxyServer {
    pub fn new(addr: String) -> Self {
        ProxyServer { addr }
    }

    pub async fn start(&self) -> crate::Result<()> {
        Ok(())
    }
}
