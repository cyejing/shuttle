use std::io::{self, ErrorKind};
use std::pin::Pin;
use std::task::Poll;

use futures::{Sink, SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

pub struct WebSocketCopyStream<T> {
    inner: WebSocketStream<T>,
}

#[allow(dead_code)]
impl<T> WebSocketCopyStream<T> {
    fn new(inner: WebSocketStream<T>) -> Self {
        Self { inner }
    }
}

impl<T> AsyncRead for WebSocketCopyStream<T>
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
                Some(Ok(Message::Close(_))) => Poll::Ready(Err(io::Error::new(
                    ErrorKind::Other,
                    "websocket close message",
                ))),
                Some(Ok(_)) => Poll::Ready(Ok(())),
                Some(Err(e)) => Poll::Ready(Err(io::Error::new(ErrorKind::Other, e))),
                None => Poll::Ready(Err(io::Error::new(ErrorKind::Other, "websocket poll none"))),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> AsyncWrite for WebSocketCopyStream<T>
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
