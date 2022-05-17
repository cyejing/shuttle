use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::broadcast::Receiver;
use tokio_rustls::TlsAcceptor;

use crate::config::Addr;
use crate::rathole::dispatcher::Dispatcher;
use crate::read_exact;
use crate::socks::ByteAddr;
use crate::store::ServerStore;
use crate::tls::make_tls_acceptor;

pub async fn start_server(addr: Addr, store: ServerStore) {
    let addr_str = addr.addr;
    let listener = TcpListener::bind(&addr_str)
        .await
        .expect("bind port failed");
    info!("Server listener addr : {}", &addr_str);
    let acceptor = if addr.ssl_enable {
        let cert_loaded = addr.cert_loaded;
        let mut key_loaded = addr.key_loaded;
        Some(make_tls_acceptor(cert_loaded, key_loaded.remove(0)))
    } else {
        None
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
            error!(
                "server [{}] accept connection err : {}",
                server.addr_str, err
            );
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
            let peer_addr = socket
                .peer_addr()
                .map(Option::Some)
                .unwrap_or_else(|_e| Option::None);

            let mut handler = ServerHandler {
                peer_addr,
                hash: Option::None,
                store: self.store.clone(),
                shutdown: self.notify_shutdown.subscribe(),
            };
            match &self.acceptor {
                Some(tls_acc) => {
                    let tls_acc = tls_acc.clone();
                    tokio::spawn(async move {
                        match tls_acc.accept(socket).await {
                            Ok(tls_ts) => {
                                if let Err(err) = handler.run(tls_ts).await {
                                    error!("tls connection error : {}", err);
                                }
                            }
                            Err(e) => {
                                error!("accept tls connection err :{}", e);
                            }
                        };
                    });
                }
                None => {
                    tokio::spawn(async move {
                        if let Err(err) = handler.run(socket).await {
                            error!("tcp connection error : {}", err);
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
    hash: Option<String>,
    store: ServerStore,
    shutdown: Receiver<()>,
}

impl ServerHandler {
    async fn run<T: AsyncRead + AsyncWrite + Unpin + Send>(
        &mut self,
        mut stream: T,
    ) -> crate::Result<()> {
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
            Err(e) => error!("detect head occur err : {}", e),
        }

        Ok(())
    }

    async fn detect_head<T: AsyncRead + AsyncWrite + Unpin>(
        &mut self,
        stream: T,
    ) -> crate::Result<ConnType> {
        let head = self.read_head(stream).await?;
        if head.len() < 56 {
            return Ok(ConnType::Proxy(head));
        }
        let hash_str = String::from_utf8(head.clone())?;

        let trojan = self.store.trojan.clone();
        let rathole = self.store.rathole.clone();

        return if trojan.password_hash.contains_key(&hash_str) {
            debug!("{} detect trojan", hash_str);
            self.hash = Some(hash_str);
            Ok(ConnType::Trojan)
        } else if rathole.password_hash.contains_key(&hash_str) {
            debug!("{} detect rathole", hash_str);
            self.hash = Some(hash_str);
            Ok(ConnType::Rathole)
        } else {
            debug!("detect proxy");
            Ok(ConnType::Proxy(head))
        };
    }

    async fn read_head<T: AsyncRead + AsyncWrite + Unpin>(
        &self,
        mut stream: T,
    ) -> crate::Result<Vec<u8>> {
        let mut buf = Vec::new();
        for _i in 0..56 {
            let u81 = stream.read_u8().await?;
            if u81 == b'\r' {
                let u82 = stream.read_u8().await?;
                if u82 == b'\n' {
                    buf.push(u81);
                    buf.push(u82);
                    return Ok(buf);
                }
            } else {
                buf.push(u81);
            }
        }
        Ok(buf)
    }

    async fn handle_trojan<T: AsyncRead + AsyncWrite + Unpin>(
        &mut self,
        stream: &mut T,
    ) -> crate::Result<()> {
        let [_cr, _cf, cmd, atyp] = read_exact!(stream, [0u8; 4])?;
        let byte_addr = ByteAddr::read_addr(stream, cmd, atyp).await?;
        info!("{:?} requested connection to {}", self.peer_addr, byte_addr);
        let socks_addr = byte_addr.to_socket_addr().await?;

        let mut cs = TcpStream::connect(socks_addr).await?;

        let [_cr, _cf] = read_exact!(stream, [0u8; 2])?;

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

    async fn handle_proxy<T: AsyncRead + AsyncWrite + Unpin>(
        &mut self,
        stream: &mut T,
        head: Vec<u8>,
    ) -> crate::Result<()> {
        info!("{:?} requested proxy local", self.peer_addr);
        let trojan = self.store.trojan.clone();
        let mut ls = TcpStream::connect(&trojan.local_addr).await?;
        ls.write_all(head.as_slice()).await?;

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

    async fn handle_rathole<T: AsyncRead + AsyncWrite + Unpin + Send>(
        &mut self,
        stream: &mut T,
    ) -> crate::Result<()> {
        let [_cr, _cf] = read_exact!(stream, [0u8; 2])?;

        let mut dispatcher =
            Dispatcher::new(stream, self.hash.clone().expect("rathole hash empty!"));
        self.store
            .set_cmd_sender(dispatcher.get_command_sender())
            .await;
        dispatcher.dispatch().await
    }
}

pub enum ConnType {
    Trojan,
    Rathole,
    Proxy(Vec<u8>),
}
