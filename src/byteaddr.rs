use itertools::Itertools;
use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use tokio::io::{AsyncRead, AsyncReadExt};

use anyhow::{anyhow, Context};
use tokio::net::lookup_host;

use crate::read_exact;

// ByteAddr Definition
#[derive(Debug, PartialEq, Eq)]
pub enum ByteAddr {
    V4(u8, u8, [u8; 4], u16),
    V6(u8, u8, [u8; 16], u16),
    Domain(u8, u8, Vec<u8>, u16), // Vec<[u8]> or Box<[u8]> or String ?
}

impl Display for ByteAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ByteAddr::V4(cmd, atyp, ip, port) => {
                write!(f, "[{},{}]:{:?}:{}", cmd, atyp, ip, port)
            }
            ByteAddr::V6(cmd, atyp, ip, port) => {
                write!(f, "[{},{}]:{:?}:{}", cmd, atyp, ip, port)
            }
            ByteAddr::Domain(cmd, atyp, domain, port) => {
                write!(
                    f,
                    "[{},{}]:{}:{}",
                    cmd,
                    atyp,
                    String::from_utf8_lossy(domain),
                    port
                )
            }
        }
    }
}

impl ByteAddr {
    pub fn to_host_string(&self) -> String {
        match self {
            ByteAddr::V4(_cmd, _atyp, ip, port) => {
                format!("{:?}:{}", ip, port)
            }
            ByteAddr::V6(_cmd, _atyp, ip, port) => {
                format!("{:?}:{}", ip, port)
            }
            ByteAddr::Domain(_cmd, _atyp, domain, port) => {
                format!("{}:{}", String::from_utf8_lossy(domain), port)
            }
        }
    }
    pub(crate) async fn to_socket_addr(&self) -> anyhow::Result<SocketAddr> {
        match self {
            ByteAddr::V4(_, _, ip, port) => {
                let sa = SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]),
                    *port,
                ));
                Ok(sa)
            }
            ByteAddr::V6(_, _, ip, port) => {
                let sa = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::from(*ip), *port, 0, 0));
                Ok(sa)
            }
            ByteAddr::Domain(_, _, domain, port) => {
                let domain_str = String::from_utf8_lossy(domain);
                let sa = lookup_host((&domain_str[..], *port))
                    .await
                    .context("Can't lookup host")?
                    .next()
                    .ok_or_else(|| anyhow!("Can't fetch DNS to the domain."))
                    .unwrap();
                Ok(sa)
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn as_bytes(&self) -> Vec<u8> {
        match self {
            ByteAddr::V4(cmd, atyp, ip, port) => {
                let mut buf = vec![*cmd, *atyp];
                buf.extend_from_slice(ip);
                buf.extend_from_slice(&port.to_be_bytes());
                debug!("byte addr as bytes:{:?}", buf);
                buf
            }
            ByteAddr::V6(cmd, atyp, ip, port) => {
                let mut buf = vec![*cmd, *atyp];
                buf.extend_from_slice(ip);
                buf.extend_from_slice(&port.to_be_bytes());
                debug!("byte addr as bytes:{:?}", buf);
                buf
            }
            ByteAddr::Domain(cmd, atyp, domain, port) => {
                let mut buf = vec![*cmd, *atyp];
                buf.push(domain.len() as u8);
                buf.extend_from_slice(domain.as_slice());
                buf.extend_from_slice(&port.to_be_bytes());
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
            socks_consts::SOCKS5_ADDR_TYPE_IPV4 => {
                let ip = read_exact!(stream, [0u8; 4])?;
                let port = read_exact!(stream, [0u8; 2])?;
                let port = (port[0] as u16) << 8 | port[1] as u16;
                ByteAddr::V4(cmd, atyp, ip, port)
            }
            socks_consts::SOCKS5_ADDR_TYPE_IPV6 => {
                let ip = read_exact!(stream, [0u8; 16])?;
                let port = read_exact!(stream, [0u8; 2])?;
                let port = (port[0] as u16) << 8 | port[1] as u16;
                ByteAddr::V6(cmd, atyp, ip, port)
            }
            socks_consts::SOCKS5_ADDR_TYPE_DOMAIN_NAME => {
                let len = read_exact!(stream, [0])?[0];
                let domain = read_exact!(stream, vec![0u8; len as usize])?;
                let port = read_exact!(stream, [0u8; 2])?;
                let port = (port[0] as u16) << 8 | port[1] as u16;

                ByteAddr::Domain(cmd, atyp, domain, port)
            }
            _ => return Err(anyhow!("unknown address type")),
        };

        Ok(addr)
    }

    pub fn from(host: &str) -> anyhow::Result<Self> {
        debug!("ByteAddr from host :{}", host);
        let (domain, port) = host
            .split(':')
            .collect_tuple()
            .context("ByteAddr parse host err")
            .unwrap();
        let port =
            atoi::atoi::<u16>(port.as_bytes()).ok_or_else(|| anyhow!("ByteAddr parse port err"))?;

        Ok(ByteAddr::Domain(
            0x01,
            0x03,
            domain.as_bytes().to_vec(),
            port,
        ))
    }
}

#[rustfmt::skip]
pub mod socks_consts {
    pub const SOCKS5_VERSION:                          u8 = 0x05;

    pub const SOCKS5_AUTH_METHOD_NONE:                 u8 = 0x00;
    pub const SOCKS5_AUTH_METHOD_GSSAPI:               u8 = 0x01;
    pub const SOCKS5_AUTH_METHOD_PASSWORD:             u8 = 0x02;
    pub const SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE:       u8 = 0xff;

    pub const SOCKS5_CMD_TCP_CONNECT:                  u8 = 0x01;
    pub const SOCKS5_CMD_TCP_BIND:                     u8 = 0x02;
    pub const SOCKS5_CMD_UDP_ASSOCIATE:                u8 = 0x03;

    pub const SOCKS5_ADDR_TYPE_IPV4:                   u8 = 0x01;
    pub const SOCKS5_ADDR_TYPE_DOMAIN_NAME:            u8 = 0x03;
    pub const SOCKS5_ADDR_TYPE_IPV6:                   u8 = 0x04;

    pub const SOCKS5_REPLY_SUCCEEDED:                  u8 = 0x00;
    pub const SOCKS5_REPLY_GENERAL_FAILURE:            u8 = 0x01;
    pub const SOCKS5_REPLY_CONNECTION_NOT_ALLOWED:     u8 = 0x02;
    pub const SOCKS5_REPLY_NETWORK_UNREACHABLE:        u8 = 0x03;
    pub const SOCKS5_REPLY_HOST_UNREACHABLE:           u8 = 0x04;
    pub const SOCKS5_REPLY_CONNECTION_REFUSED:         u8 = 0x05;
    pub const SOCKS5_REPLY_TTL_EXPIRED:                u8 = 0x06;
    pub const SOCKS5_REPLY_COMMAND_NOT_SUPPORTED:      u8 = 0x07;
    pub const SOCKS5_REPLY_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;
}
