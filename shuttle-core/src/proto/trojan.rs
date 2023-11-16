use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use anyhow::{anyhow, Context};
use bytes::{BufMut, BytesMut};
use socks5_proto::{Address, Command};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{peekable::PeekableStream, CRLF};

/// Trojan request
///
/// ```plain
/// +-----------------------+---------+----------------+---------+----------+
/// | hex(SHA224(password)) |  CRLF   | Trojan Request |  CRLF   | Payload  |
/// +-----------------------+---------+----------------+---------+----------+
/// |          56           | X'0D0A' |    Variable    | X'0D0A' | Variable |
/// +-----------------------+---------+----------------+---------+----------+
///
/// where Trojan Request is a SOCKS5-like request:
///
/// +-----+------+----------+----------+
/// | CMD | ATYP | DST.ADDR | DST.PORT |
/// +-----+------+----------+----------+
/// |  1  |  1   | Variable |    2     |
/// +-----+------+----------+----------+
///
/// ```
#[derive(Clone, Debug)]
pub struct Request {
    pub hash: String,
    pub command: Command,
    pub address: Address,
}

impl Request {
    const ATYP_IPV4: u8 = 0x01;
    const ATYP_FQDN: u8 = 0x03;
    const ATYP_IPV6: u8 = 0x04;

    pub const fn new(hash: String, command: Command, address: Address) -> Self {
        Self {
            hash,
            command,
            address,
        }
    }

    pub async fn peek_hash<T>(r: &mut PeekableStream<T>) -> anyhow::Result<(String, usize)>
    where
        T: AsyncRead + Unpin,
    {
        let mut buf = Vec::new();
        let mut len = 0;
        for _i in 0..56 {
            let b1 = r.peek_u8().await.context("Trojan peek u8 failed")?;
            len += 1;
            if b1 == b'\r' {
                let b2 = r.peek_u8().await.context("Trojan peek u8 failed")?;
                len += 1;
                if b2 == b'\n' {
                    buf.push(b1);
                    buf.push(b2);
                    break;
                }
            } else {
                buf.push(b1);
            }
        }

        let hash_str = String::from_utf8(buf).context("Trojan hash to string failed")?;
        Ok((hash_str, len))
    }

    pub async fn read_from<R>(r: &mut R) -> anyhow::Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf: [u8; 56] = [0; 56];
        let len = r
            .read(&mut buf[..])
            .await
            .context("Trojan read hash failed")?;
        if len != 56 {
            return Err(anyhow!("The Request not Trojan"));
        }

        let hash = String::from_utf8_lossy(&buf[..]).to_string();

        let _crlf = r.read_u16().await?;

        let (cmd, addr) = Self::read_address_from(r).await?;

        let _crlf = r.read_u16();

        Ok(Self::new(hash, cmd, addr))
    }

    pub async fn read_address_from<R>(r: &mut R) -> anyhow::Result<(Command, Address)>
    where
        R: AsyncRead + Unpin,
    {
        let cmd = r.read_u8().await?;
        let cmd = Command::try_from(cmd).map_err(|cmd| anyhow!("Unknown cmd {cmd}"))?;

        let atyp = r.read_u8().await?;

        match atyp {
            Self::ATYP_IPV4 => {
                let mut buf = [0; 6];
                r.read_exact(&mut buf).await?;

                let addr = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);

                let port = u16::from_be_bytes([buf[4], buf[5]]);

                let addr = Address::SocketAddress(SocketAddr::from((addr, port)));
                Ok((cmd, addr))
            }
            Self::ATYP_FQDN => {
                let len = r.read_u8().await? as usize;

                let mut buf = vec![0; len + 2];
                r.read_exact(&mut buf).await?;

                let port = u16::from_be_bytes([buf[len], buf[len + 1]]);
                buf.truncate(len);

                let addr = Address::DomainAddress(buf, port);
                Ok((cmd, addr))
            }
            Self::ATYP_IPV6 => {
                let mut buf = [0; 18];
                r.read_exact(&mut buf).await?;

                let addr = Ipv6Addr::new(
                    u16::from_be_bytes([buf[0], buf[1]]),
                    u16::from_be_bytes([buf[2], buf[3]]),
                    u16::from_be_bytes([buf[4], buf[5]]),
                    u16::from_be_bytes([buf[6], buf[7]]),
                    u16::from_be_bytes([buf[8], buf[9]]),
                    u16::from_be_bytes([buf[10], buf[11]]),
                    u16::from_be_bytes([buf[12], buf[13]]),
                    u16::from_be_bytes([buf[14], buf[15]]),
                );

                let port = u16::from_be_bytes([buf[16], buf[17]]);

                let addr = Address::SocketAddress(SocketAddr::from((addr, port)));
                Ok((cmd, addr))
            }
            atyp => Err(anyhow!("invalid type {atyp}")),
        }
    }

    pub async fn write_to<W>(&self, w: &mut W) -> anyhow::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = BytesMut::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        w.write_all(&buf).await.context("Trojan Write buf failed")?;

        Ok(())
    }

    pub fn write_to_buf<B: BufMut>(&self, buf: &mut B) {
        buf.put_slice(self.hash.as_bytes());
        buf.put_slice(&CRLF);
        buf.put_u8(u8::from(self.command));
        self.write_to_buf_address(buf);
        buf.put_slice(&CRLF);
    }
    pub fn write_to_buf_address<B: BufMut>(&self, buf: &mut B) {
        match &self.address {
            Address::SocketAddress(SocketAddr::V4(addr)) => {
                buf.put_u8(Self::ATYP_IPV4);
                buf.put_slice(&addr.ip().octets());
                buf.put_u16(addr.port());
            }
            Address::SocketAddress(SocketAddr::V6(addr)) => {
                buf.put_u8(Self::ATYP_IPV6);
                for seg in addr.ip().segments() {
                    buf.put_u16(seg);
                }
                buf.put_u16(addr.port());
            }
            Address::DomainAddress(addr, port) => {
                buf.put_u8(Self::ATYP_FQDN);
                buf.put_u8(addr.len() as u8);
                buf.put_slice(addr);
                buf.put_u16(*port);
            }
        }
    }

    pub fn serialized_len(&self) -> usize {
        56 + 2 + 1 + self.address.serialized_len() + 2
    }
}
