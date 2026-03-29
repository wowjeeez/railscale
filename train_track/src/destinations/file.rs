use std::io::{BufWriter, Write};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::fs::File;
use std::time::SystemTime;
use bytes::Bytes;
use tokio::io::AsyncWrite;
use crate::{Frame, StreamDestination};

pub struct FileDestination<T: Frame, FS: FrameSerializer<T> = DefaultFrameSerializer<T>> {
    writer: BufWriter<File>,
    serializer: FS,
    _t: PhantomData<T>,
}

impl<T: Frame, FS: FrameSerializer<T>> FileDestination<T, FS> {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { _t: PhantomData, writer: BufWriter::new(file), serializer: FS::init() })
    }
}

pub trait FrameSerializer<T: Frame>: Send + Sync {
    fn serialize(&self, frame: &T) -> Result<impl Into<Bytes>, std::io::Error>;
    fn init() -> Self;
}

pub struct DefaultFrameSerializer<T: Frame>(PhantomData<T>);

impl<T: Frame + Send + Sync> FrameSerializer<T> for DefaultFrameSerializer<T> {
    fn serialize(&self, frame: &T) -> Result<impl Into<Bytes>, std::io::Error> {
        Ok(String::from_utf8_lossy(frame.as_bytes()).to_string())
    }

    fn init() -> Self {
        Self(PhantomData)
    }
}

#[async_trait::async_trait]
impl<T: Frame + Sync, FS: FrameSerializer<T>> StreamDestination for FileDestination<T, FS> {
    type Frame = T;
    type Error = std::io::Error;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error> {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        write!(self.writer, "--- {}.{:06} {} ---\n",
            ts.as_secs(), ts.subsec_micros(),
            String::from_utf8_lossy(routing_frame.as_bytes()),
        )
    }

    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error> {
        let bytes: Bytes = self.serializer.serialize(&frame)?.into();
        self.writer.write_all(bytes.as_ref())?;
        self.writer.write_all(b"\n")
    }

    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.writer.write_all(&bytes)
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, _client: &mut W) -> Result<u64, Self::Error> {
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(0)
    }
}
