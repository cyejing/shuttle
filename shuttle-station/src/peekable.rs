use async_trait::async_trait;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[async_trait]
pub trait AsyncPeek {
    async fn peek_u8(&mut self) -> anyhow::Result<u8>;

    async fn peek(&mut self, buf: &mut [u8]) -> anyhow::Result<usize>;

    fn drain(&mut self) -> Option<Vec<u8>>;
}

#[derive(Debug)]
pub struct PeekableStream<S> {
    inner: S,
    buf: Option<Vec<u8>>,
}

#[async_trait]
impl<S> AsyncPeek for PeekableStream<S>
where
    S: AsyncRead + Unpin + Send,
{
    async fn peek_u8(&mut self) -> anyhow::Result<u8> {
        let u8 = self.inner.read_u8().await?;
        if let Some(ref mut peek_buf) = self.buf {
            peek_buf.push(u8)
        } else {
            self.buf = Some(vec![u8]);
        }
        Ok(u8)
    }

    async fn peek(&mut self, buf: &mut [u8]) -> anyhow::Result<usize> {
        let n = self.inner.read(buf).await?;
        let mut buf = buf[0..n].to_vec();
        if let Some(ref mut peek_buf) = self.buf {
            peek_buf.append(&mut buf)
        } else {
            self.buf = Some(buf);
        }
        Ok(n)
    }

    fn drain(&mut self) -> Option<Vec<u8>> {
        self.buf.take()
    }
}

impl<S> AsyncRead for PeekableStream<S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        if let Some(peek_buf) = me.buf.take() {
            buf.put_slice(&peek_buf);
            Poll::Ready(Ok(()))
        } else {
            Pin::new(&mut me.inner).poll_read(cx, buf)
        }
    }
}

impl<S> AsyncWrite for PeekableStream<S>
where
    S: AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl<S> PeekableStream<S> {
    pub fn new(inner: S) -> Self {
        PeekableStream { inner, buf: None }
    }
}
