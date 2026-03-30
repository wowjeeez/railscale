use bytes::{Bytes, BytesMut};
use tokio::io::{self, AsyncWrite, AsyncWriteExt};
use crate::atom::frame::Frame;

pub struct BatchWriter<W = ()> {
    buf: BytesMut,
    writer: W,
}

impl<W> BatchWriter<W> {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn push_bytes(&mut self, data: Bytes) {
        self.buf.extend_from_slice(&data);
    }

    pub fn push_frames<F: Frame>(&mut self, frames: impl IntoIterator<Item = F>) {
        for frame in frames {
            self.buf.extend_from_slice(frame.as_bytes());
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }
}

impl BatchWriter<()> {
    pub fn new() -> Self {
        Self { buf: BytesMut::new(), writer: () }
    }

    pub fn take(&mut self) -> Bytes {
        self.buf.split().freeze()
    }
}

impl<W: AsyncWrite + Unpin> BatchWriter<W> {
    pub fn with_writer(writer: W) -> Self {
        Self { buf: BytesMut::new(), writer }
    }

    pub async fn flush_all(&mut self) -> io::Result<usize> {
        let data = self.buf.split();
        let len = data.len();
        if len > 0 {
            self.writer.write_all(&data).await?;
            self.writer.flush().await?;
        }
        Ok(len)
    }
}
