use anyhow::Context;
use bytes::{BufMut, BytesMut};
use rand::{Rng, RngCore};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug)]
pub struct Padding {
    len: u16,
}

impl Padding {
    pub async fn read_from<R>(stream: &mut R) -> anyhow::Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        let len = stream.read_u16().await?;
        let mut buf = vec![0; len as usize];
        stream.read_exact(&mut buf).await?;
        Ok(Padding { len })
    }

    pub async fn write_to<W>(&self, w: &mut W) -> anyhow::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = BytesMut::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        w.write_all(&buf)
            .await
            .context("Padding Write buf failed")?;

        Ok(())
    }

    pub fn write_to_buf<B: BufMut>(&self, buf: &mut B) {
        let rand_buf = self.rand_buf();
        buf.put_u16(self.len);
        buf.put_slice(&rand_buf);
    }

    pub fn serialized_len(&self) -> usize {
        2 + self.len as usize
    }

    pub fn rand_buf(&self) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let mut buf = vec![0; self.len as usize];
        rng.fill_bytes(&mut buf);
        buf
    }
}

impl Default for Padding {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        let len = rng.gen_range(100..3000);
        Self { len }
    }
}
