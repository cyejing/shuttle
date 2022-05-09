use std::net::SocketAddr;
use std::time::Duration;

use log::{debug, error, info};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio::sync::broadcast::Receiver;
use tokio_rustls::TlsAcceptor;

use crate::config::Addr;
use crate::{read_exact};
use crate::rathole::connection::{Connection, ConnectionHolder};
use crate::socks::ByteAddr;
use crate::store::ServerStore;
use crate::tls::make_tls_acceptor;

pub async fn start_server(addr: Addr, store: ServerStore) {
    let addr_str = addr.addr;
    let listener = TcpListener::bind(&addr_str).await.expect("bind port failed");
    info!("Server listener addr : {}", &addr_str);
    let acceptor = if addr.ssl_enable {
        let cert_loaded = addr.cert_loaded;
        let mut key_loaded = addr.key_loaded;
        Option::Some(make_tls_acceptor(cert_loaded, key_loaded.remove(0)))
    } else {
        Option::None
    };

    let (notify_shutdown, _) = broadcast::channel(1);
    let mut server = Server {
        addr_str,
        listener,
        store,
        acceptor,
        notify_shutdown,
    };
    tokio::spawn(async move {
        if let Err(err) = server.run().await {
            error!("server [{}] accept connection err : {}", server.addr_str, err);
        }
    });
}

struct Server {
    addr_str: String,
    listener: TcpListener,
    store: ServerStore,
    acceptor: Option<TlsAcceptor>,
    notify_shutdown: broadcast::Sender<()>,
}

impl Server {
    async fn run(&mut self) -> crate::Result<()> {
        loop {
            let socket = self.accept().await?;
            let peer_addr = socket.peer_addr()
                .map(Option::Some)
                .unwrap_or_else(|_e| Option::None);

            let mut handler = ServerHandler {
                peer_addr,
                store: self.store.clone(),
                shutdown: self.notify_shutdown.subscribe(),
            };
            match &self.acceptor {
                Option::Some(tls_acc) => {
                    let tls_acc = tls_acc.clone();
                    match tls_acc.accept(socket).await {
                        Ok(tls_ts) => {
                            tokio::spawn(async move {
                                if let Err(err) = handler.run(tls_ts).await {
                                    error!("tls connection error : {}",err);
                                }
                            });
                        }
                        Err(e) => {
                            error!("accept tls connection err :{}", e);
                        }
                    };
                }
                Option::None => {
                    tokio::spawn(async move {
                        if let Err(err) = handler.run(socket).await {
                            error!("tcp connection error : {}",err);
                        }
                    });
                }
            }
        }
    }

    async fn accept(&mut self) -> crate::Result<TcpStream> {
        let mut backoff = 1;

        loop {
            match self.listener.accept().await {
                Ok((socket, _)) => return Ok(socket),
                Err(err) => {
                    if backoff > 64 {
                        return Err(err.into());
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(backoff)).await;

            backoff *= 2;
        }
    }
}


struct ServerHandler {
    peer_addr: Option<SocketAddr>,
    store: ServerStore,
    shutdown: Receiver<()>,
}

impl ServerHandler {
    async fn run<T: AsyncRead + AsyncWrite + Unpin>(&mut self, mut stream: T) -> crate::Result<()> {
        match self.detect_head(&mut stream).await {
            Ok(ConnType::Trojan) => {
                self.handle_trojan(&mut stream).await?;
            }
            Ok(ConnType::Rathole) => {
                self.handle_rathole(&mut stream).await?;
                debug!("detect rathole");
            }
            Ok(ConnType::Proxy(head)) => {
                self.handle_proxy(&mut stream, head).await?;
            }
            Err(e) => error!("detect head occur err : {}",e),
        }

        Ok(())
    }

    async fn detect_head<T: AsyncRead + AsyncWrite + Unpin>(&self, mut stream: T) -> crate::Result<ConnType> {
        let head = read_exact!(stream, [0u8; 56])?;
        let hash_str = String::from_utf8_lossy(&head);
        let hash_str = hash_str.as_ref();

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
            Ok(ConnType::Proxy(head))
        };
    }

    async fn handle_trojan<T: AsyncRead + AsyncWrite + Unpin>(&mut self, stream: &mut T) -> crate::Result<()> {
        let [_cr, _cf, cmd, atyp] = read_exact!(stream,[0u8; 4])?;
        let byte_addr = ByteAddr::read_addr(stream, cmd, atyp).await?;
        info!("{:?} requested connection to {}",self.peer_addr,byte_addr);
        let socks_addr = byte_addr.to_socket_addr().await?;

        let mut cs = TcpStream::connect(socks_addr).await?;

        let [_cr, _cf] = read_exact!(stream,[0u8; 2])?;

        tokio::select! {
            res = tokio::io::copy_bidirectional(stream, &mut cs) => {
                match res{
                    Ok(s)=>debug!("trojan io copy end {:?}", s),
                    Err(e)=>error!("trojan io copy err {}", e),
                }
            },
            _ = self.shutdown.recv() => {
                debug!("recv shutdown signal");
            }
        }
        Ok(())
    }

    async fn handle_proxy<T: AsyncRead + AsyncWrite + Unpin>(&mut self, stream: &mut T, head: [u8; 56]) -> crate::Result<()> {
        info!("{:?} requested proxy local",self.peer_addr);
        let trojan = self.store.trojan.clone();
        let mut ls = TcpStream::connect(&trojan.local_addr).await?;
        ls.write_all(&head).await?;

        tokio::select! {
            res = tokio::io::copy_bidirectional(stream, &mut ls) => {
                match res{
                    Ok(s)=>debug!("proxy io copy end {:?}", s),
                    Err(e)=>error!("proxy io copy err {}", e),
                }
            },
            _ = self.shutdown.recv() => {
                debug!("recv shutdown signal");
            }
        }
        Ok(())
    }

    async fn handle_rathole<T: AsyncRead + AsyncWrite + Unpin>(&mut self, stream: &mut T) -> crate::Result<()> {
        let (_se,re) = mpsc::channel(128);
        let conn = Connection::new(stream);
        let mut connection_holder = ConnectionHolder::new(conn, re);
        connection_holder.run().await;
        Ok(())
    }
}

pub enum ConnType {
    Trojan,
    Rathole,
    Proxy([u8; 56]),
}


