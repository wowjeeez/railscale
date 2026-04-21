use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf};
use tokio_stream::{Stream, StreamExt};
use train_track::*;

struct TestFrame(Bytes, bool);
impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { if self.1 { Some(&self.0) } else { None } }
}

struct OneShotSource;
impl StreamSource for OneShotSource {
    type ReadHalf = ReadHalf<DuplexStream>;
    type WriteHalf = WriteHalf<DuplexStream>;
    type Error = std::io::Error;

    async fn accept(&self) -> Result<(Self::ReadHalf, Self::WriteHalf), Self::Error> {
        let (client, mut server) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            server.write_all(b"GET / HTTP/1.1\r\nHost: test\r\n\r\nbody").await.unwrap();
            server.shutdown().await.unwrap();
        });
        Ok(tokio::io::split(client))
    }
}

struct MockParser;
impl<S: AsyncRead + Send + Unpin> FrameParser<S> for MockParser {
    type Frame = TestFrame;
    type Error = std::io::Error;
    fn parse(&mut self, _stream: S) -> impl Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        tokio_stream::iter(vec![
            Ok(ParsedData::Parsed(TestFrame(Bytes::from_static(b"GET / HTTP/1.1"), true))),
            Ok(ParsedData::Parsed(TestFrame(Bytes::from_static(b"Host: test"), false))),
            Ok(ParsedData::Passthrough(Bytes::from_static(b"body"))),
        ])
    }
}

struct NoopPipeline;
impl FramePipeline for NoopPipeline {
    type Frame = TestFrame;
    fn process(&self, frame: Self::Frame) -> Self::Frame { frame }
}

struct CountingDestination {
    written: Arc<AtomicUsize>,
    empty: tokio::io::Empty,
}
impl CountingDestination {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let w = Arc::new(AtomicUsize::new(0));
        (Self { written: w.clone(), empty: tokio::io::empty() }, w)
    }
}

#[async_trait::async_trait]
impl StreamDestination for CountingDestination {
    type Error = std::io::Error;
    type ResponseReader = tokio::io::Empty;

    async fn write(&mut self, _bytes: Bytes) -> Result<(), Self::Error> {
        self.written.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn response_reader(&mut self) -> &mut tokio::io::Empty {
        &mut self.empty
    }
}

struct MockRouter;

#[async_trait::async_trait]
impl DestinationRouter for MockRouter {
    type Destination = CountingDestination;

    async fn route(&self, _routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        let (dest, _) = CountingDestination::new();
        Ok(dest)
    }
}

#[tokio::test]
async fn pipeline_drives_connection() {
    let source = OneShotSource;
    let (read_half, _write_half) = source.accept().await.unwrap();
    let mut parser = MockParser;
    let pipeline = NoopPipeline;
    let (mut dest, written) = CountingDestination::new();

    let frames = parser.parse(read_half);
    let mut frames = pin!(frames);

    while let Some(Ok(item)) = frames.next().await {
        match item {
            ParsedData::Passthrough(bytes) => {
                dest.write(bytes).await.unwrap();
            }
            ParsedData::Parsed(frame) => {
                let frame = pipeline.process(frame);
                dest.write(frame.into_bytes()).await.unwrap();
            }
        }
    }

    assert_eq!(written.load(Ordering::SeqCst), 3);
}
