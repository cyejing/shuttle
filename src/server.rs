use anyhow::{anyhow, Context};
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
        .context(format!("Can't bind server port {}", addr_str))
        .unwrap();
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
                "Server [{}] accept connection err : {}",
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
    async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            let socket = self.accept().await.context("Server Can't accept conn")?;
            let peer_addr = socket
                .peer_addr()
                .map(Option::Some)
                .unwrap_or_else(|_e| Option::None);

            let mut handler = ServerHandler {
                peer_addr,
                hash: None,
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
                                    error!("handler tls connection error : {}", err);
                                }
                            }
                            Err(e) => {
                                error!("accept tls connection err : {}", e);
                            }
                        };
                    });
                }
                None => {
                    tokio::spawn(async move {
                        if let Err(err) = handler.run(socket).await {
                            error!("handler tcp connection error : {}", err);
                        }
                    });
                }
            }
        }
    }

    async fn accept(&mut self) -> anyhow::Result<TcpStream> {
        let mut backoff = 100;

        loop {
            match self.listener.accept().await {
                Ok((socket, _)) => return Ok(socket),
                Err(err) => {
                    if backoff > 6400 {
                        return Err(err.into());
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(backoff)).await;

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
    ) -> anyhow::Result<()> {
        match self.detect_head(&mut stream).await {
            Ok(ConnType::Trojan) => self.handle_trojan(&mut stream).await,
            Ok(ConnType::Rathole) => self.handle_rathole(&mut stream).await,
            Ok(ConnType::Proxy(head)) => self.handle_proxy(&mut stream, head).await,
            Err(e) => Err(e).context("Can't detect head"),
        }
    }

    async fn detect_head<T: AsyncRead + AsyncWrite + Unpin>(
        &mut self,
        stream: T,
    ) -> anyhow::Result<ConnType> {
        let head = self.read_head(stream).await.context("Can't read head")?;
        if head.len() < 56 {
            return Ok(ConnType::Proxy(head));
        }
        let hash_str = String::from_utf8(head.clone()).context("Can't stringing hash head")?;

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
    ) -> anyhow::Result<Vec<u8>> {
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
    ) -> anyhow::Result<()> {
        let [_cr, _cf, cmd, atyp] = read_exact!(stream, [0u8; 4])?;
        let byte_addr = ByteAddr::read_addr(stream, cmd, atyp)
            .await
            .context("Trojan Can't read ByteAddr")?;
        info!("Requested connection {:?} to {}", self.peer_addr, byte_addr);
        let socks_addr = byte_addr
            .to_socket_addr()
            .await
            .context("Can't cover ByteAddr to SocksAddr")?;

        let mut cs = TcpStream::connect(socks_addr)
            .await
            .context(format!("Trojan can't connect addr {}", socks_addr))?;

        let [_cr, _cf] = read_exact!(stream, [0u8; 2])?;

        tokio::select! {
            r = tokio::io::copy_bidirectional(stream, &mut cs) => {r.context("Trojan io copy end")?;},
            _ = self.shutdown.recv() => debug!("recv shutdown signal"),
        }
        Ok(())
    }

    async fn handle_proxy<T: AsyncRead + AsyncWrite + Unpin>(
        &mut self,
        stream: &mut T,
        head: Vec<u8>,
    ) -> anyhow::Result<()> {
        info!("{:?} requested proxy local", self.peer_addr);
        let trojan = self.store.trojan.clone();
        let mut ls = TcpStream::connect(&trojan.local_addr)
            .await
            .context(format!("Proxy can't connect addr {}", &trojan.local_addr))?;

        ls.write_all(head.as_slice())
            .await
            .context("Proxy can't write prefetch head")?;

        tokio::select! {
            r = tokio::io::copy_bidirectional(stream, &mut ls) => {r.context("Proxy io copy end")?;},
            _ = self.shutdown.recv() => debug!("recv shutdown signal"),
        }
        Ok(())
    }

    async fn handle_rathole<T: AsyncRead + AsyncWrite + Unpin + Send>(
        &mut self,
        stream: &mut T,
    ) -> anyhow::Result<()> {
        let [_cr, _cf] = read_exact!(stream, [0u8; 2])?;
        let hash_str = self
            .hash
            .clone()
            .ok_or_else(|| anyhow!("rathole hash empty!"))?;
        let (mut dispatcher, cs) = Dispatcher::new(stream, hash_str.clone());
        self.store
            .set_cmd_sender(cs)
            .await;
        let r = tokio::select! {
            r = dispatcher.dispatch() => r,
            _ = self.shutdown.recv() => {
                debug!("recv shutdown signal");
                Ok(())
            },
        };
        // drop(dispatcher);
        self.store.remove_cmd_sender(&hash_str).await;
        r
    }
}

pub enum ConnType {
    Trojan,
    Rathole,
    Proxy(Vec<u8>),
}
