use std::io::Write;
use std::marker::PhantomData;
use std::path::PathBuf;
use bytes::{BufMut, Bytes};
use memmap2::MmapMut;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use crate::{Frame, StreamDestination};

pub struct FileDestination<T: Frame, FS: FrameSerializer<T> = DefaultFrameSerializer<T>> {
    mmap_mut: MmapMut,
    packet_dest: Option<String>,
    serializer: FS,
    _t: PhantomData<T>
}

impl<T: Frame, FS: FrameSerializer<T>> FileDestination<T, FS> {
    pub async fn new(path: PathBuf) -> tokio::io::Result<Self> { ;
        let file = OpenOptions::new().write(true).create(true).open(path).await?;
        let mut mmap = unsafe { MmapMut::map_mut(&file)? };
        Ok(Self { _t: PhantomData, packet_dest: None, mmap_mut: mmap, serializer: FS::init()})
    }
}

pub trait FrameSerializer<T: Frame> : Send + Sync  {
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
impl<T: Frame + Sync + ToString, FS: FrameSerializer<T>> StreamDestination for FileDestination<T, FS> {
    type Frame = T;
    type Error = std::io::Error;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error> {
        self.packet_dest = Some(String::from_utf8_lossy(routing_frame.as_bytes()).to_string());
        Ok(())
    }

    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error> {
        let bytes = self.serializer.serialize(&frame)?.into();
        self.mmap_mut.writer().write(bytes.as_ref())?;
        Ok(())
    }

    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error>{
        self.mmap_mut.writer().write(format!("<Body len={}/>", bytes.len()).as_bytes())?;
        Ok(())
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        Ok(0)
    }
}