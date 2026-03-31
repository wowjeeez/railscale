use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_stream::Stream;
use crate::atom::frame::{Frame, ParsedData};
use crate::atom::parser::FrameParser;
use crate::core::pipeline::FramePipeline;

const RAW_CHUNK_SIZE: usize = 8192;

pub struct RawFrame(Bytes);

impl RawFrame {
    pub fn new(data: Bytes) -> Self {
        Self(data)
    }
}

impl Frame for RawFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

pub struct RawParser;

impl RawParser {
    pub fn new() -> Self {
        Self
    }
}

impl<S: AsyncRead + Send + Unpin + 'static> FrameParser<S> for RawParser {
    type Frame = RawFrame;
    type Error = std::io::Error;

    fn parse(&mut self, stream: S) -> impl Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        async_stream::stream! {
            let mut reader = stream;
            let mut buf = BytesMut::with_capacity(RAW_CHUNK_SIZE);
            loop {
                buf.clear();
                buf.reserve(RAW_CHUNK_SIZE);
                let n = reader.read_buf(&mut buf).await?;
                if n == 0 {
                    return;
                }
                yield Ok(ParsedData::Parsed(RawFrame::new(buf.split().freeze())));
            }
        }
    }
}

pub struct RawPipeline;

impl FramePipeline for RawPipeline {
    type Frame = RawFrame;
    fn process(&self, frame: Self::Frame) -> Self::Frame { frame }
}
