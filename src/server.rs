use std::sync::Arc;

use log::{debug, error};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::config::{Addr, ServerConfig, ServerStore};
use crate::tls::make_tls_acceptor;

pub struct TlsServer {
    pub addr: Addr,
    pub config: Arc<ServerConfig>,
}

impl TlsServer {
    pub fn new(addr: Addr,config: Arc<ServerConfig>) -> TlsServer {
        TlsServer {
            addr,
            config,
        }
    }

    pub async fn start(self) -> crate::Result<()> {
        let addr = &self.addr.addr;
        let cert_loaded = self.addr.cert_loaded;
        let mut key_loaded = self.addr.key_loaded;

        let store = ServerStore::from(self.config);


        Ok(())
    }
}


pub struct ServerStream<T> {
    ss: T,
    store: ServerStore,
}
