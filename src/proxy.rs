use std::io::{self, ErrorKind};
use std::pin::Pin;
use std::task::Poll;

use anyhow::{anyhow, Context};
use futures::{Sink, SinkExt, StreamExt};
use itertools::Itertools;
use tokio::io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::byteaddr::ByteAddr;
use crate::proxy::DialStream::{TCP, TLS, WS};
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
            error!("Socks run err : {:?}", e);
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
    ts.peek(&mut fb)
        .await
        .context("Can't peek byte by proxy connection")?;

    if fb[0] == socks_consts::SOCKS5_VERSION {
        debug!(
            "Socks proxy connection {:?} to {:?}",
            ts.peer_addr().ok(),
            ts.local_addr().ok()
        );
        let mut ss = SocksStream { ts, dial };
        ss.handle().await
    } else {
        debug!(
            "Http proxy connection {:?} to {:?}",
            ts.peer_addr().ok(),
            ts.local_addr().ok()
        );
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
        info!("Requested http connection to: {addr}", addr = addr,);
        match self
            .dial
            .dial(&addr)
            .await
            .context("Http can't dial remote addr")?
        {
            TCP(mut rts) => {
                if http.connect {
                    ts.write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes()).await?;
                } else {
                    rts.write_all(http.buf.as_slice()).await?;
                }
                debug!("Start copy stream {}", &addr);
                copy_bidirectional(&mut rts, &mut ts).await.ok();
            }
            TLS(mut rts) => {
                if http.connect {
                    ts.write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes()).await?;
                } else {
                    rts.write_all(http.buf.as_slice()).await?;
                }
                debug!("Start copy stream {}", &addr);
                copy_bidirectional(&mut rts, &mut ts).await.ok();
            }
            WS(_) => {
                info!("ws handle")
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
            .dial(&addr)
            .await
            .context("Socks can't dial remote addr")?
        {
            TCP(mut rts) => {
                self.reply(socks_consts::SOCKS5_REPLY_SUCCEEDED)
                    .await
                    .context("Socks send succeeded reply")?;
                debug!(
                    "Start tcp io copy {:?} <=> {:?}",
                    rts.peer_addr(),
                    self.ts.peer_addr()
                );
                copy_bidirectional(&mut rts, &mut self.ts).await.ok();
            }
            TLS(mut rts) => {
                self.reply(socks_consts::SOCKS5_REPLY_SUCCEEDED)
                    .await
                    .context("Socks send succeeded reply")?;
                debug!(
                    "Start tls io copy {:?} <=> {:?}",
                    rts.get_ref().0.peer_addr(),
                    self.ts.peer_addr()
                );
                copy_bidirectional(&mut rts, &mut self.ts).await.ok();
            }
            WS(mut ws) => {
                self.reply(socks_consts::SOCKS5_REPLY_SUCCEEDED)
                    .await
                    .context("Socks send succeeded reply")?;
                debug!("Start websocket io copy");

                copy_bidirectional(&mut ws, &mut self.ts).await.ok();
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

        info!("Requested socks connection to: {addr}", addr = addr,);

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
pub type WSStream = WebSocketTrojanStream<MaybeTlsStream<TcpStream>>;

pub enum DialStream {
    TCP(Box<TcpStream>),
    TLS(Box<TlsStream<TcpStream>>),
    WS(Box<WSStream>),
}

#[derive(Clone, Debug)]
pub enum Dial {
    Direct,
    Trojan(String, String, bool, bool),
    WebSocket(String, String, bool, bool),
}

impl Dial {
    pub async fn dial(&self, ba: &ByteAddr) -> anyhow::Result<DialStream> {
        match self {
            Dial::Direct => {
                let addr_str = ba
                    .to_socket_addr()
                    .await
                    .context("ByteAddr can't cover to socket addr")?;
                match TcpStream::connect(addr_str)
                    .await
                    .context(format!("Socks can't connect addr {}", addr_str))
                {
                    Ok(tts) => {
                        debug!("Dail mode direct success {}", addr_str);
                        Ok(TCP(Box::new(tts)))
                    }
                    Err(e) => {
                        error!("Can't connect socket server {}, {:?}", addr_str, e);
                        Err(e)
                    }
                }
            }
            Dial::Trojan(remote_addr, hash, ssl_enable, invalid_certs) => {
                match TcpStream::connect(remote_addr)
                    .await
                    .context(format!("Trojan can't connect remote {}", remote_addr))
                {
                    Err(e) => {
                        error!("Can't connect trojan server {}, {:?}", remote_addr, e);
                        Err(anyhow!(e))
                    }
                    Ok(mut tts) => {
                        debug!("Dial mode trojan success {}", remote_addr);
                        let mut buf: Vec<u8> = vec![];
                        buf.extend_from_slice(hash.as_bytes());
                        buf.extend_from_slice(&CRLF);
                        buf.extend_from_slice(ba.as_bytes().as_slice());
                        buf.extend_from_slice(&CRLF);

                        if *ssl_enable {
                            let server_name = make_server_name(remote_addr.as_str())?;
                            let mut ssl_tts = make_tls_connector(*invalid_certs)
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
            Dial::WebSocket(remote_addr, hash, _ssl_enable, _invalid_certs) => {
                match connect_async(remote_addr)
                    .await
                    .context(format!("WebSocket can't connect remote {}", remote_addr))
                {
                    Err(e) => {
                        error!("Can't connect trojan server {}, {:?}", remote_addr, e);
                        Err(anyhow!(e))
                    }
                    Ok((mut ws, _)) => {
                        let mut buf: Vec<u8> = vec![];
                        buf.extend_from_slice(hash.as_bytes());
                        buf.extend_from_slice(&CRLF);
                        buf.extend_from_slice(ba.as_bytes().as_slice());
                        buf.extend_from_slice(&CRLF);

                        ws.send(Message::Binary(buf))
                            .await
                            .context("WebSocket can't send")?;

                        Ok(WS(Box::new(WebSocketTrojanStream::new(ws))))
                    }
                }
            }
        }
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

pub struct WebSocketTrojanStream<T> {
    inner: WebSocketStream<T>,
}

impl<T> WebSocketTrojanStream<T> {
    fn new(inner: WebSocketStream<T>) -> Self {
        Self { inner }
    }
}

impl<T> AsyncRead for WebSocketTrojanStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let me = self.get_mut();
        let next = me.inner.poll_next_unpin(cx);
        match next {
            Poll::Ready(t) => match t {
                Some(Ok(Message::Binary(b))) => {
                    buf.put_slice(b.as_slice());
                    Poll::Ready(Ok(()))
                }
                Some(Ok(Message::Close(_))) => {
                    info!("websocket close message");
                    Poll::Ready(Err(io::Error::from(ErrorKind::Other)))
                }
                Some(Ok(_)) => Poll::Ready(Ok(())),
                Some(Err(_e)) => Poll::Ready(Err(io::Error::from(ErrorKind::Other))),
                None => Poll::Ready(Err(io::Error::from(ErrorKind::Other))),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> AsyncWrite for WebSocketTrojanStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let me = self.get_mut();
        let binary = Vec::from(buf);
        let len = binary.len();
        me.inner.start_send_unpin(Message::Binary(binary)).ok();

        Poll::Ready(Ok(len))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        let me = self.get_mut();
        let stream = &mut me.inner;
        match Pin::new(stream).poll_flush(cx) {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
            Poll::Ready(Err(_)) => Poll::Ready(Err(io::Error::from(ErrorKind::Other))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        let me = self.get_mut();
        let stream = &mut me.inner;
        match Pin::new(stream).poll_close(cx) {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
            Poll::Ready(Err(_)) => Poll::Ready(Err(io::Error::from(ErrorKind::Other))),
            Poll::Pending => Poll::Pending,
        }
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::byteaddr::ByteAddr;

    #[test]
    fn test_host_str() {
        let ba = ByteAddr::from("baidu.com:443").unwrap();
        println!("host: {}", ba.to_host_string());
        assert_eq!("baidu.com:443", ba.to_host_string());
        let ba1 = ByteAddr::from("1.2.5.2:222").unwrap();
        println!("host: {}", ba1.to_host_string());
        assert_eq!("1.2.5.2:222", ba1.to_host_string())
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
