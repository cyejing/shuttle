use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{lookup_host, TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;

use crate::common::{consts, socks_consts};
use crate::config::ClientConfig;
use crate::read_exact;
use crate::socks::DialStream::{TCP, TLS};
use crate::tls::{make_server_name, make_tls_connector};

pub async fn start_socks(cc: ClientConfig, dial: Arc<dyn DialRemote>) {
    let listener = TcpListener::bind(&cc.sock_addr)
        .await
        .context(format!("Can't Listen socks addr {}", &cc.sock_addr))
        .unwrap();
    info!("Listen for socks connections @ {}", &cc.sock_addr);
    let socks = Socks { listener, dial };
    tokio::spawn(async move {
        if let Err(e) = socks.run().await {
            error!("Socks run err : {}", e);
        }
    });
}

struct Socks {
    listener: TcpListener,
    dial: Arc<dyn DialRemote>,
}

impl Socks {
    pub async fn run(&self) -> anyhow::Result<()> {
        loop {
            let (ts, _) = self.listener.accept().await?;
            let dial = self.dial.clone();
            let mut ss = SocksStream { ts };
            tokio::spawn(async move {
                if let Err(e) = ss.handle(dial).await {
                    error!("Socks stream handle err : {}", e);
                };
            });
        }
    }
}

struct SocksStream {
    ts: TcpStream,
}

impl SocksStream {
    async fn handle(&mut self, dr: Arc<dyn DialRemote>) -> anyhow::Result<()> {
        self.handshake().await.context("Socks Can't handshake")?;

        let addr = self.read_request().await.context("Socks Can't read request")?;

        match dr.dial(addr).await.context("Socks can't dial remote addr")? {
            TCP(mut rts) => {
                self.reply(socks_consts::SOCKS5_REPLY_SUCCEEDED).await?;
                copy_bidirectional(&mut rts, &mut self.ts).await.context("Socks io copy err")?;
            }
            TLS(mut rts) => {
                self.reply(socks_consts::SOCKS5_REPLY_SUCCEEDED).await?;
                copy_bidirectional(&mut rts, &mut self.ts).await.context("Socks io copy err")?;
            }
        };

        Ok(())
    }

    async fn handshake(&mut self) -> anyhow::Result<()> {
        let [version, methods_len] = read_exact!(self.ts, [0u8; 2])?;
        debug!(
            "Handshake headers: [version: {version}, methods len: {len}]",
            version = version,
            len = methods_len,
        );
        if version != socks_consts::SOCKS5_VERSION {
            return Err(anyhow!("unknown socks5 version"));
        }
        let methods = read_exact!(self.ts, vec![0u8; methods_len as usize])?;
        debug!("Methods supported sent by the client: {:?}", &methods);

        self.ts
            .write_all(&[
                socks_consts::SOCKS5_VERSION,
                socks_consts::SOCKS5_AUTH_METHOD_NONE,
            ])
            .await?;
        Ok(())
    }

    async fn read_request(&mut self) -> anyhow::Result<ByteAddr> {
        let [version, cmd, rsv, address_type] = read_exact!(self.ts, [0u8; 4])?;
        debug!(
            "Request: [version: {version}, command: {cmd}, rev: {rsv}, address_type: {address_type}]",
            version = version,
            cmd = cmd,
            rsv = rsv,
            address_type = address_type,
        );

        if version != socks_consts::SOCKS5_VERSION {
            return Err(anyhow!("unknown socks5 version"));
        }

        let addr = ByteAddr::read_addr(&mut self.ts, cmd, address_type).await?;

        info!("Requested connection to: {addr}", addr = addr,);

        Ok(addr)
    }

    async fn reply(&mut self, resp: u8) -> anyhow::Result<()> {
        let buf = vec![
            socks_consts::SOCKS5_VERSION,
            resp,
            0,
            socks_consts::SOCKS5_ADDR_TYPE_IPV4,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        debug!("Reply [buf={buf:?}]", buf = buf,);
        self.ts.write_all(buf.as_ref()).await.context("Socks can't reply, write reply err")?;
        Ok(())
    }
}

pub enum DialStream {
    TCP(Box<TcpStream>),
    TLS(Box<TlsStream<TcpStream>>),
}

#[async_trait]
pub trait DialRemote: Send + Sync {
    async fn dial(&self, ba: ByteAddr) -> anyhow::Result<DialStream>;
}

pub struct SocksDial {}

#[async_trait]
impl DialRemote for SocksDial {
    async fn dial(&self, ba: ByteAddr) -> anyhow::Result<DialStream> {
        let addr_str = ba.to_socket_addr().await.context("ByteAddr can't cover to socket addr")?;
        let tts = TcpStream::connect(addr_str).await.context(format!("Socks can't connect addr {}", addr_str))?;
        Ok(DialStream::TCP(Box::new(tts)))
    }
}

pub struct TrojanDial {
    remote: String,
    hash: String,
    domain: String,
    ssl_enable: bool,
}

impl TrojanDial {
    pub fn new(remote: String, hash: String, ssl_enable: bool) -> TrojanDial {
        let domain = remote.clone();
        let domain = domain
            .split(':')
            .next()
            .unwrap_or_else(|| panic!("domain parse error : {}", remote));
        debug!("Parse domain is : {}", domain);
        TrojanDial {
            remote,
            hash,
            domain: String::from(domain),
            ssl_enable,
        }
    }
}

#[async_trait]
impl DialRemote for TrojanDial {
    async fn dial(&self, ba: ByteAddr) -> anyhow::Result<DialStream> {
        let mut tts = TcpStream::connect(&self.remote).await.context(format!("Trojan can't connect remote {}", &self.remote))?;
        let mut buf: Vec<u8> = vec![];
        buf.extend_from_slice(self.hash.as_bytes());
        buf.extend_from_slice(&consts::CRLF);
        buf.extend_from_slice(ba.as_bytes().as_slice());
        buf.extend_from_slice(&consts::CRLF);

        if self.ssl_enable {
            let server_name = make_server_name(self.domain.as_str())?;
            let mut ssl_tts = make_tls_connector().connect(server_name, tts).await.context("Trojan can't connect tls")?;
            ssl_tts.write_all(buf.as_slice()).await.context("Can't write trojan")?;
            Ok(TLS(Box::new(ssl_tts)))
        } else {
            tts.write_all(buf.as_slice()).await.context("Can't write trojan")?;
            Ok(TCP(Box::new(tts)))
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ByteAddr {
    V4(u8, u8, [u8; 4], [u8; 2]),
    V6(u8, u8, [u8; 16], [u8; 2]),
    Domain(u8, u8, Vec<u8>, [u8; 2]), // Vec<[u8]> or Box<[u8]> or String ?
}

impl Display for ByteAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ByteAddr::V4(cmd, atyp, ip, port) => {
                let cov_port = (port[0] as u16) << 8 | port[1] as u16;
                write!(f, "[{},{}]:{:?}:{}", cmd, atyp, ip, cov_port)
            }
            ByteAddr::V6(cmd, atyp, ip, port) => {
                let cov_port = (port[0] as u16) << 8 | port[1] as u16;
                write!(f, "[{},{}]:{:?}:{}", cmd, atyp, ip, cov_port)
            }
            ByteAddr::Domain(cmd, atyp, domain, port) => {
                let cov_port = (port[0] as u16) << 8 | port[1] as u16;
                write!(
                    f,
                    "[{},{}]:{}:{}",
                    cmd,
                    atyp,
                    String::from_utf8_lossy(domain),
                    cov_port
                )
            }
        }
    }
}

impl ByteAddr {
    pub(crate) async fn to_socket_addr(&self) -> anyhow::Result<SocketAddr> {
        match self {
            ByteAddr::V4(_, _, ip, port) => {
                let cov_port = (port[0] as u16) << 8 | port[1] as u16;
                let sa = SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]),
                    cov_port,
                ));
                Ok(sa)
            }
            ByteAddr::V6(_, _, ip, port) => {
                let cov_port = (port[0] as u16) << 8 | port[1] as u16;
                let sa = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::from(*ip), cov_port, 0, 0));
                Ok(sa)
            }
            ByteAddr::Domain(_, _, domain, port) => {
                let cov_port = (port[0] as u16) << 8 | port[1] as u16;
                let domain_str = String::from_utf8_lossy(domain);
                let sa = lookup_host((&domain_str[..], cov_port))
                    .await
                    .context("Can't lookup host")?
                    .next()
                    .ok_or(anyhow!("Can't fetch DNS to the domain."))
                    .unwrap();
                Ok(sa)
            }
        }
    }

    pub(crate) fn as_bytes(&self) -> Vec<u8> {
        match self {
            ByteAddr::V4(cmd, atyp, ip, port) => {
                let mut buf = vec![*cmd, *atyp];
                buf.extend_from_slice(ip);
                buf.extend_from_slice(port);
                debug!("byte addr as bytes:{:?}", buf);
                buf
            }
            ByteAddr::V6(cmd, atyp, ip, port) => {
                let mut buf = vec![*cmd, *atyp];
                buf.extend_from_slice(ip);
                buf.extend_from_slice(port);
                debug!("byte addr as bytes:{:?}", buf);
                buf
            }
            ByteAddr::Domain(cmd, atyp, domain, port) => {
                let mut buf = vec![*cmd, *atyp];
                buf.push(domain.len() as u8);
                buf.extend_from_slice(domain.as_slice());
                buf.extend_from_slice(port);
                debug!(
                    "byte addr as bytes:{:?} , domain:{}",
                    buf,
                    String::from_utf8_lossy(domain)
                );
                buf
            }
        }
    }

    pub async fn read_addr<T>(stream: &mut T, cmd: u8, atyp: u8) -> anyhow::Result<ByteAddr>
        where
            T: AsyncRead + Unpin,
    {
        let addr = match atyp {
            socks_consts::SOCKS5_ADDR_TYPE_IPV4 => ByteAddr::V4(
                cmd,
                atyp,
                read_exact!(stream, [0u8; 4])?,
                read_exact!(stream, [0u8; 2])?,
            ),
            socks_consts::SOCKS5_ADDR_TYPE_IPV6 => ByteAddr::V6(
                cmd,
                atyp,
                read_exact!(stream, [0u8; 16])?,
                read_exact!(stream, [0u8; 2])?,
            ),
            socks_consts::SOCKS5_ADDR_TYPE_DOMAIN_NAME => {
                let len = read_exact!(stream, [0])?[0];
                let domain = read_exact!(stream, vec![0u8; len as usize])?;

                ByteAddr::Domain(cmd, atyp, domain, read_exact!(stream, [0u8; 2])?)
            }
            _ => return Err(anyhow!("unknown address type")),
        };

        Ok(addr)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::socks::ByteAddr;

    #[test]
    fn test_as_byte_addr() {
        let byte_addr = ByteAddr::V4(0x01, 0x01, [0x01, 0x01, 0x01, 0x01], [0x00, 0x50]);
        let bs = byte_addr.as_bytes();
        assert_eq!(bs.as_slice(), [1, 1, 1, 1, 1, 1, 0, 80]);
    }

    #[tokio::test]
    async fn test_byte_addr_to_sa() {
        let byte_addr = ByteAddr::V4(0x01, 0x01, [0x01, 0x01, 0x01, 0x01], [0x00, 0x50]);
        let sa = byte_addr.to_socket_addr().await.unwrap();
        assert!(sa.is_ipv4());
        assert_eq!(format!("{}", sa.ip()), "1.1.1.1");
        assert_eq!(sa.port(), 80)
    }

    #[tokio::test]
    async fn test_byte_addr_read() {
        let vec = vec![0x01, 0x01, 0x01, 0x01, 0x00, 0x50];
        let mut buf = Cursor::new(vec);
        let addr = ByteAddr::read_addr(&mut buf, 0x01, 0x01).await.unwrap();
        assert_eq!(
            addr,
            ByteAddr::V4(0x01, 0x01, [0x01, 0x01, 0x01, 0x01], [0x00, 0x50])
        )
    }
}
