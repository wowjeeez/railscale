use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::fs::File;
use bytes::Bytes;
use tokio::io::AsyncWrite;
use crate::StreamDestination;

pub struct FileDestination {
    writer: BufWriter<File>,
    serializer: Box<dyn FrameSerializer + Send>,
}

impl FileDestination {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { writer: BufWriter::new(file), serializer: Box::new(DefaultFrameSerializer) })
    }

    pub fn with_serializer(path: PathBuf, serializer: Box<dyn FrameSerializer + Send>) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { writer: BufWriter::new(file), serializer })
    }
}

pub trait FrameSerializer: Send + Sync {
    fn serialize(&self, bytes: &[u8]) -> Result<Bytes, std::io::Error>;
}

pub struct DefaultFrameSerializer;

impl FrameSerializer for DefaultFrameSerializer {
    fn serialize(&self, bytes: &[u8]) -> Result<Bytes, std::io::Error> {
        Ok(Bytes::from(String::from_utf8_lossy(bytes).to_string()))
    }
}

#[async_trait::async_trait]
impl StreamDestination for FileDestination {
    type Error = std::io::Error;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        let serialized = self.serializer.serialize(&bytes)?;
        self.writer.write_all(serialized.as_ref())?;
        self.writer.write_all(b"\n")
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, _client: &mut W) -> Result<u64, Self::Error> {
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(0)
    }
}
