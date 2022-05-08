use std::future::Future;
use std::net::SocketAddr;

use log::{debug, error, info};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

use crate::config::{Addr};
use crate::read_exact;
use crate::socks::ByteAddr;
use crate::store::ServerStore;
use crate::tls::make_tls_acceptor;

pub async fn start_tcp_server(addr: String, store: ServerStore, shutdown: impl Future) {
    let lis = TcpListener::bind(&addr).await.expect("bind port failed");
    info!("Tcp Server listener addr : {}", &addr);
    loop {
        match lis.accept().await {
            Ok((ts, _sd)) => {
                let peer_addr = ts.peer_addr()
                    .map(Option::Some)
                    .unwrap_or_else(|_e| Option::None);
                let handler = ServerHandler::new(store.clone(), peer_addr);
                tokio::spawn(handler.handle(ts));
            }
            Err(e) => {
                error!("accept connection err,{}", e)
            }
        }
    }
}

pub async fn start_tls_server(addr: Addr, store: ServerStore, shutdown: impl Future) {
    let addr_str = addr.addr;
    let cert_loaded = addr.cert_loaded;
    let mut key_loaded = addr.key_loaded;

    let acceptor = make_tls_acceptor(cert_loaded, key_loaded.remove(0));

    let lis = TcpListener::bind(&addr_str).await.expect("bind port failed");
    info!("Tls Server listener addr : {}", &addr_str);
    loop {
        match lis.accept().await {
            Ok((ts, _sd)) => {
                let peer_addr = ts.peer_addr()
                    .map(Option::Some)
                    .unwrap_or_else(|_e| Option::None);
                let tls_acc = acceptor.clone();
                match tls_acc.accept(ts).await {
                    Ok(tls_ts) => {
                        let handler = ServerHandler::new(store.clone(), peer_addr);
                        tokio::spawn(handler.handle(tls_ts));
                    }
                    Err(e) => {
                        error!("accept tls connection err,{}", e);
                    }
                }
            }
            Err(e) => {
                error!("accept connection err,{}", e);
            }
        }
    }
}

#[derive(Debug)]
struct Listener {
    store: ServerStore,

    listener: TcpListener,
    notify_shutdown: broadcast::Sender<()>,

}

pub struct ServerHandler {
    peer_addr: Option<SocketAddr>,
    store: ServerStore,
}

impl ServerHandler {
    pub fn new(store: ServerStore, peer_addr: Option<SocketAddr>) -> Self {
        ServerHandler {
            store,
            peer_addr,
        }
    }
    pub async fn handle<T: AsyncRead + AsyncWrite + Unpin>(self, mut stream: T) -> crate::Result<()> {
        let head = read_exact!(stream, [0u8; 56])?;
        let hash_str = String::from_utf8_lossy(&head);

        match self.detect_head(hash_str.as_ref()).await {
            Ok(ConnType::Trojan) => {
                self.handle_trojan(&mut stream).await?;
            }
            Ok(ConnType::Rathole) => {
                self.handle_rathole(&mut stream).await?;
                debug!("detect rathole");
            }
            Ok(ConnType::Proxy) => {
                self.handle_proxy(&mut stream, head).await?;
            }
            Err(e) => error!("detect head occur err : {}",e),
        }

        Ok(())
    }

    async fn detect_head(&self, hash_str: &str) -> crate::Result<ConnType> {
        let trojan = self.store.trojan.clone();
        let rathole = self.store.rathole.clone();

        return if trojan.password_hash.contains_key(hash_str) {
            debug!("{} detect trojan", hash_str);
            Ok(ConnType::Trojan)
        } else if rathole.password_hash.contains_key(hash_str) {
            debug!("{} detect rathole", hash_str);
            Ok(ConnType::Rathole)
        } else {
            debug!("detect proxy");
            Ok(ConnType::Proxy)
        };
    }

    async fn handle_trojan<T: AsyncRead + AsyncWrite + Unpin>(&self, stream: &mut T) -> crate::Result<()> {
        let [_cr, _cf, cmd, atyp] = read_exact!(stream,[0u8; 4])?;
        let byte_addr = ByteAddr::read_addr(stream, cmd, atyp).await?;
        info!("{:?} requested connection to {}",self.peer_addr,byte_addr);
        let socks_addr = byte_addr.to_socket_addr().await?;

        let mut cs = TcpStream::connect(socks_addr).await?;

        let [_cr, _cf] = read_exact!(stream,[0u8; 2])?;

        tokio::io::copy_bidirectional(stream, &mut cs).await?;

        Ok(())
    }

    async fn handle_proxy<T: AsyncRead + AsyncWrite + Unpin>(&self, stream: &mut T, head: [u8; 56]) -> crate::Result<()> {
        info!("{:?} requested proxy local",self.peer_addr);
        let trojan = self.store.trojan.clone();
        let mut ls = TcpStream::connect(&trojan.local_addr).await?;
        ls.write_all(&head).await?;

        tokio::io::copy_bidirectional(stream, &mut ls).await?;
        Ok(())
    }

    async fn handle_rathole<T: AsyncRead + AsyncWrite + Unpin>(&self, _stream: &mut T) -> crate::Result<()> {

        Ok(())
    }
}

pub enum ConnType {
    Trojan,
    Rathole,
    Proxy,
}


