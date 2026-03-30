use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::fs::File;
use bytes::Bytes;
use tokio::io::AsyncWrite;
use crate::StreamDestination;

pub struct FileDestination {
    writer: BufWriter<File>,
}

impl FileDestination {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        Ok(Self { writer: BufWriter::new(file) })
    }
}

#[async_trait::async_trait]
impl StreamDestination for FileDestination {
    type Error = std::io::Error;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.writer.write_all(&bytes)
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, _client: &mut W) -> Result<u64, Self::Error> {
        self.writer.flush()?;
        Ok(0)
    }
}
