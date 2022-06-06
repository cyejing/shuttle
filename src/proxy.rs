use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::Poll;

use anyhow::{anyhow, Context};
use itertools::Itertools;
use tokio::io::{
    self, copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf,
};
use tokio::net::{lookup_host, TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;

use crate::proxy::DialStream::{TCP, TLS};
use crate::tls::{make_server_name, make_tls_connector};
use crate::{read_exact, CRLF};

pub async fn start_proxy(addr: &str, dial: Dial) {
    let listener = TcpListener::bind(addr)
        .await
        .context(format!("Can't Listen socks addr {}", addr))
        .unwrap();

    info!("Listen for socks connections @ {}", addr);
    tokio::spawn(async move {
        if let Err(e) = serve(listener, dial).await {
            error!("Socks run err : {}", e);
        }
    });
}

async fn serve(listener: TcpListener, dial: Dial) -> anyhow::Result<()> {
    loop {
        let (ts, _) = listener.accept().await?;
        let d = dial.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(ts, d).await {
                error!("Socks stream handle err : {:?}", e);
            };
        });
    }
}

async fn handle(ts: TcpStream, dial: Dial) -> anyhow::Result<()> {
    let mut fb = [0u8];
    ts.peek(&mut fb).await?;

    if fb[0] == socks_consts::SOCKS5_VERSION {
        let mut ss = SocksStream { ts, dial };
        ss.handle().await
    } else {
        let mut hs = HttpStream { dial };
        hs.handle(ts).await
    }
}

struct HttpStream {
    dial: Dial,
}

struct SocksStream {
    ts: TcpStream,
    dial: Dial,
}

impl HttpStream {
    async fn handle(&mut self, mut ts: TcpStream) -> anyhow::Result<()> {
        let mut http = Http::new();
        http.read(&mut ts).await?;

        let addr = ByteAddr::from(&http.host.ok_or_else(|| anyhow!("Http Host empty"))?)?;
        debug!("Http proxy addr: {}", addr);
        match self
            .dial
            .dial(addr)
            .await
            .context("Http can't dial remote addr")?
        {
            TCP(mut rts) => {
                if http.connect {
                    ts.write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes()).await?;
                } else {
                    rts.write_all(http.buf.as_slice()).await?;
                }
                copy_bidirectional(&mut rts, &mut ts)
                    .await
                    .context("Http io copy err")?;
            }
            TLS(mut rts) => {
                if http.connect {
                    ts.write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes()).await?;
                } else {
                    rts.write_all(http.buf.as_slice()).await?;
                }
                copy_bidirectional(&mut rts, &mut ts)
                    .await
                    .context("Http io copy err")?;
            }
        };
        Ok(())
    }
}

impl SocksStream {
    async fn handle(&mut self) -> anyhow::Result<()> {
        self.handshake().await.context("Socks Can't handshake")?;

        let addr = self
            .read_request()
            .await
            .context("Socks Can't read request")?;

        match self
            .dial
            .dial(addr)
            .await
            .context("Socks can't dial remote addr")?
        {
            TCP(mut rts) => {
                self.reply(socks_consts::SOCKS5_REPLY_SUCCEEDED).await?;
                copy_bidirectional(&mut rts, &mut self.ts)
                    .await
                    .context("Socks io copy err")?;
            }
            TLS(mut rts) => {
                self.reply(socks_consts::SOCKS5_REPLY_SUCCEEDED).await?;
                copy_bidirectional(&mut rts, &mut self.ts)
                    .await
                    .context("Socks io copy err")?;
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
        self.ts
            .write_all(buf.as_ref())
            .await
            .context("Socks can't reply, write reply err")?;
        Ok(())
    }
}

pub enum DialStream {
    TCP(Box<TcpStream>),
    TLS(Box<TlsStream<TcpStream>>),
}

#[derive(Clone, Debug)]
pub enum Dial {
    Direct,
    Trojan(String, String, bool),
}

impl Dial {
    pub async fn dial(&self, ba: ByteAddr) -> anyhow::Result<DialStream> {
        match self {
            Dial::Direct => {
                let addr_str = ba
                    .to_socket_addr()
                    .await
                    .context("ByteAddr can't cover to socket addr")?;
                let tts = TcpStream::connect(addr_str)
                    .await
                    .context(format!("Socks can't connect addr {}", addr_str))?;
                Ok(DialStream::TCP(Box::new(tts)))
            }
            Dial::Trojan(remote_addr, hash, ssl_enable) => {
                let mut tts = TcpStream::connect(remote_addr)
                    .await
                    .context(format!("Trojan can't connect remote {}", remote_addr))?;
                let mut buf: Vec<u8> = vec![];
                buf.extend_from_slice(hash.as_bytes());
                buf.extend_from_slice(&CRLF);
                buf.extend_from_slice(ba.as_bytes().as_slice());
                buf.extend_from_slice(&CRLF);

                if *ssl_enable {
                    let server_name = make_server_name(remote_addr.as_str())?;
                    let mut ssl_tts = make_tls_connector()
                        .connect(server_name, tts)
                        .await
                        .context("Trojan can't connect tls")?;
                    ssl_tts
                        .write_all(buf.as_slice())
                        .await
                        .context("Can't write trojan")?;
                    Ok(TLS(Box::new(ssl_tts)))
                } else {
                    tts.write_all(buf.as_slice())
                        .await
                        .context("Can't write trojan")?;
                    Ok(TCP(Box::new(tts)))
                }
            }
        }
    }
}

// ByteAddr Definition
#[derive(Debug, PartialEq)]
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

struct Http {
    buf: Vec<u8>,
    host: Option<String>,
    connect: bool,
}

impl Http {
    fn new() -> Self {
        Http {
            buf: Vec::new(),
            host: None,
            connect: false,
        }
    }

    async fn read<T: AsyncRead + Unpin>(&mut self, ts: &mut T) -> anyhow::Result<()> {
        let fline = Self::read_line(ts).await?;
        self.buf.append(&mut fline.clone());
        let (m, h, v) = String::from_utf8(fline)?
            .split(' ')
            .map(|s| s.to_lowercase().trim().to_string())
            .collect_tuple()
            .context("Http split method")
            .unwrap();
        debug!("http: {} {} {}", m, h, v);

        if m == "connect" {
            self.connect = true;
        }

        loop {
            let mut line = Self::read_line(ts).await?;
            if line.len() == 2 {
                self.buf.append(&mut line);
                break;
            }
            let (k, mut v) = String::from_utf8(line.clone())?
                .split(": ")
                .map(|s| s.to_lowercase().trim().to_string())
                .collect_tuple()
                .context("Http split header")
                .unwrap();

            debug!("heaeder: {} : {}", k, v);
            if k == "host" {
                if !v.contains(':') {
                    v.push_str(":80");
                    self.host = Some(v);
                } else {
                    self.host = Some(v);
                }
            }

            if k != "accept-encoding"
                && k != "connection"
                && k != "proxy-connection"
                && k != "proxy-authenticate"
                && k != "proxy-authorization"
            {
                self.buf.append(&mut line.clone());
            }
        }

        Ok(())
    }

    async fn read_line<T: AsyncRead + Unpin>(ts: &mut T) -> anyhow::Result<Vec<u8>> {
        let mut line = Vec::new();
        loop {
            let u81 = ts.read_u8().await?;
            line.push(u81);
            if u81 == b'\r' {
                let u82 = ts.read_u8().await?;
                if u82 == b'\n' {
                    line.push(u82);
                    return Ok(line);
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct SyncSocket {
    stream: Arc<Mutex<TcpStream>>,
}

impl SyncSocket {
    pub fn new(stream: TcpStream) -> Self {
        SyncSocket {
            stream: Arc::new(Mutex::new(stream)),
        }
    }
}

impl AsyncRead for SyncSocket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut *self.stream.lock().unwrap()), cx, buf)
    }
}

impl AsyncWrite for SyncSocket {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        AsyncWrite::poll_write(Pin::new(&mut *self.stream.lock().unwrap()), cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        AsyncWrite::poll_flush(Pin::new(&mut *self.stream.lock().unwrap()), cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut *self.stream.lock().unwrap()), cx)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::proxy::ByteAddr;

    #[test]
    fn test_from_str() {
        let ba = ByteAddr::from("baidu.com:443").unwrap();
        println!("ba : {}", ba);
    }

    #[test]
    fn test_as_byte_addr() {
        let byte_addr = ByteAddr::V4(0x01, 0x01, [0x01, 0x01, 0x01, 0x01], 80);
        let bs = byte_addr.as_bytes();
        assert_eq!(bs.as_slice(), [1, 1, 1, 1, 1, 1, 0, 80]);
    }

    #[tokio::test]
    async fn test_byte_addr_to_sa() {
        let byte_addr = ByteAddr::V4(0x01, 0x01, [0x01, 0x01, 0x01, 0x01], 80);
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
        assert_eq!(addr, ByteAddr::V4(0x01, 0x01, [0x01, 0x01, 0x01, 0x01], 80))
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
