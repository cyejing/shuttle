use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;

use log::{debug, error, info};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{lookup_host, TcpListener, TcpStream};
use tokio_rustls::{rustls, TlsConnector};
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::OwnedTrustAnchor;

use crate::common::{consts, socks_consts};
use crate::config::ClientConfig;
use crate::read_exact;

pub struct Socks {
    pub cc: ClientConfig,
}

impl Socks {
    pub fn new(cc: ClientConfig) -> Socks {
        Socks {
          cc
        }
    }

    pub async fn start(self) -> crate::Result<()> {
        let listener = TcpListener::bind(&self.cc.sock_addr).await?;
        info!("Listen for socks connections @ {}", &self.cc.sock_addr);

        loop {
            let res_acc = listener.accept().await;
            let remote = self.cc.remote_addr.clone();
            let hash = self.cc.hash.clone();
            match res_acc {
                Ok(acc) => {
                    tokio::spawn(async move {
                        if let Err(e) =
                        SocksStream::new(acc, remote, hash).handle().await {
                            error!("socks stream handle err {:?}",e);
                        };
                    });
                }
                Err(e) => error!("accept err {}",e),
            };
        }
    }
}

pub struct SocksStream {
    ts: TcpStream,
    sd: SocketAddr,
    remote: String,
    hash: String,
}

impl SocksStream {
    pub fn new((ts, sd): (TcpStream, SocketAddr),
               remote: String,hash: String) -> SocksStream {
        SocksStream {
            ts,
            sd,
            remote,
            hash,
        }
    }

    pub async fn handle(&mut self) -> crate::Result<()> {
        info!("Socks connection {}", self.sd);
        self.handshake().await?;

        let addr = self.read_request().await?;

        // let mut rts = SocksStream::connect_remote(addr).await?;

        let mut rts =
            SocksStream::send_trojan(&self.remote, &self.hash, addr).await?;

        self.reply(socks_consts::SOCKS5_REPLY_SUCCEEDED).await?;

        copy_bidirectional(&mut rts, &mut self.ts).await?;

        Ok(())
    }

    async fn handshake(&mut self) -> crate::Result<()> {
        let [version, methods_len] = read_exact!(self.ts, [0u8; 2])?;
        debug!(
            "Handshake headers: [version: {version}, methods len: {len}]",
            version = version,
            len = methods_len,
        );
        if version != socks_consts::SOCKS5_VERSION {
            return Err(String::from("unknown socks5 version").into());
        }
        let methods = read_exact!(self.ts, vec![0u8; methods_len as usize])?;
        debug!("methods supported sent by the client: {:?}", &methods);

        self.ts.write(&[socks_consts::SOCKS5_VERSION, socks_consts::SOCKS5_AUTH_METHOD_NONE]).await?;
        Ok(())
    }

    async fn read_request(&mut self) -> crate::Result<ByteAddr> {
        let [version, cmd, rsv, address_type] = read_exact!(self.ts, [0u8; 4])?;
        debug!(
            "Request: [version: {version}, command: {cmd}, rev: {rsv}, address_type: {address_type}]",
            version = version,
            cmd = cmd,
            rsv = rsv,
            address_type = address_type,
        );

        if version != socks_consts::SOCKS5_VERSION {
            return Err(String::from("unknown socks5 version").into());
        }

        let addr = ByteAddr::read_addr(&mut self.ts, cmd, address_type).await?;

        debug!(
            "Read address: [addr: {addr:?}]",
            addr = addr,
        );

        Ok(addr)
    }

    async fn reply(&mut self, resp: u8) -> crate::Result<()> {
        let buf = vec![socks_consts::SOCKS5_VERSION, resp, 0,
                       socks_consts::SOCKS5_ADDR_TYPE_IPV4, 0, 0, 0, 0, 0, 0];
        debug!(
            "Reply [buf={buf:?}]",
             buf = buf,
        );
        self.ts.write(buf.as_ref()).await?;
        Ok(())
    }

    #[allow(dead_code)]
    async fn connect_remote(ba: ByteAddr) -> crate::Result<TcpStream> {
        let addr_str = ba.to_addr_str().await?;
        let tts = TcpStream::connect(addr_str).await?;
        Ok(tts)
    }


    async fn send_trojan(remote: &String, hash: &String, ba: ByteAddr) -> crate::Result<TlsStream<TcpStream>> {
        let mut tts = TcpStream::connect(remote).await?;
        info!("send trojan remote={}",remote);

        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(
            |ta| {
                OwnedTrustAnchor::from_subject_spki_name_constraints(
                    ta.subject,
                    ta.spki,
                    ta.name_constraints,
                )
            },
        ));
        let config = tokio_rustls::rustls::client::ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));
        let server_name = rustls::ServerName::try_from("mini.cyejing.cn")?;
        info!("{:?}",server_name);
        let mut tls_tts = connector.connect(server_name, tts).await?;

        let mut buf: Vec<u8> = vec![];
        buf.extend_from_slice(hash.as_bytes());
        buf.extend_from_slice(&consts::CRLF);
        buf.extend_from_slice(ba.as_bytes().as_slice());
        buf.extend_from_slice(&consts::CRLF);

        tls_tts.write(buf.as_slice()).await?;

        Ok(tls_tts)
    }
}


#[derive(Debug)]
pub enum ByteAddr {
    V4(u8, u8, [u8; 4], [u8; 2]),
    V6(u8, u8, [u8; 16], [u8; 2]),
    Domain(u8, u8, Vec<u8>, [u8; 2]), // Vec<[u8]> or Box<[u8]> or String ?
}

impl ByteAddr {
    #[allow(dead_code)]
    pub(crate) async fn to_addr_str(&self) -> crate::Result<SocketAddr> {
        match self {
            ByteAddr::V4(_, _, ip, port) => {
                let cov_port = (port[0] as u16) << 8 | port[1] as u16;
                let sa = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]), cov_port));
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
                    .await?
                    .next()
                    .ok_or("Can't fetch DNS to the domain.")
                    .unwrap();
                Ok(sa)
            }
        }
    }
}

impl ByteAddr {
    pub(crate) fn as_bytes(&self) -> Vec<u8> {
        match self {
            ByteAddr::V4(cmd, atyp, ip, port) => {
                let mut buf = vec![*cmd, *atyp];
                buf.extend_from_slice(ip);
                buf.extend_from_slice(port);
                debug!("byte addr as bytes:{:?}",buf);
                buf
            },
            ByteAddr::V6(cmd, atyp, ip, port) => {
                let mut buf = vec![*cmd, *atyp];
                buf.extend_from_slice(ip);
                buf.extend_from_slice(port);
                debug!("byte addr as bytes:{:?}",buf);
                buf
            },
            ByteAddr::Domain(cmd,atyp,domain,port) => {
                debug!("byte addr domain:{}",String::from_utf8_lossy(domain));
                let mut buf = vec![*cmd, *atyp];
                buf.push(domain.len() as u8);
                buf.extend_from_slice(domain.as_slice());
                buf.extend_from_slice(port);
                debug!("byte addr as bytes:{:?}",buf);
                buf
            }
        }
    }

    pub async fn read_addr<T>(stream: &mut T, cmd: u8, atyp: u8) -> crate::Result<ByteAddr>
        where T: AsyncRead + AsyncReadExt + Unpin {
        let addr = match atyp {
            socks_consts::SOCKS5_ADDR_TYPE_IPV4 => {
                ByteAddr::V4(cmd, atyp, read_exact!(stream, [0u8; 4])?, read_exact!(stream, [0u8; 2])?)
            }
            socks_consts::SOCKS5_ADDR_TYPE_IPV6 => {
                ByteAddr::V6(cmd, atyp, read_exact!(stream, [0u8; 16])?, read_exact!(stream, [0u8; 2])?)
            }
            socks_consts::SOCKS5_ADDR_TYPE_DOMAIN_NAME => {
                let len = read_exact!(stream, [0])?[0];
                let domain = read_exact!(stream, vec![0u8; len as usize])?;

                ByteAddr::Domain(cmd, atyp, domain, read_exact!(stream, [0u8; 2])?)
            }
            _ => return Err(String::from("unknown address type").into()),
        };

        Ok(addr)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::net::ToSocketAddrs;
    use std::sync::Arc;
    use log::info;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio_rustls::{rustls, TlsConnector};
    use tokio_rustls::rustls::OwnedTrustAnchor;

    use crate::socks::ByteAddr;

    #[test]
    fn test_byte_addr() {
        let byte_addr = ByteAddr::V4(0x01, 0x02, [0x01, 0x01, 0x01, 0x01], [0x01, 0x01]);
        let bs = byte_addr.as_bytes();
        println!("{:?}", bs);
        assert_eq!(bs.as_slice(), [1, 2, 1, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn test_stream() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let remote = "mini.cyejing.cn:4843";
            let mut tts = TcpStream::connect(remote).await.unwrap();
            info!("send trojan remote={}",remote);

            let mut root_cert_store = rustls::RootCertStore::empty();
            root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(
                |ta| {
                    OwnedTrustAnchor::from_subject_spki_name_constraints(
                        ta.subject,
                        ta.spki,
                        ta.name_constraints,
                    )
                },
            ));
            let config = tokio_rustls::rustls::client::ClientConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(root_cert_store)
                .with_no_client_auth();

            let connector = TlsConnector::from(Arc::new(config));
            let server_name = rustls::ServerName::try_from("mini.cyejing.cn").unwrap();
            info!("{:?}",server_name);
            let mut tls_tts = connector.connect(server_name, tts).await.unwrap();

            tls_tts.write_all(&b"GET / HTTP/1.1\r\n\
            Connection: Close\r\n
            Accept: */*\r\n"[..]).await.unwrap();

            let mut res = String::new();
            tls_tts.read_to_string(&mut res).await.unwrap();
            println!("{}",res)
        });
    }
}
