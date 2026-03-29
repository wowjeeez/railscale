use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWriteExt, DuplexStream};
use tokio_stream::{Stream, StreamExt};
use train_track::*;

struct TestFrame(Bytes, bool);
impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn is_routing_frame(&self) -> bool { self.1 }
}

struct OneShotSource;
impl StreamSource for OneShotSource {
    type Stream = DuplexStream;
    type Error = std::io::Error;
    async fn accept(&self) -> Result<Self::Stream, Self::Error> {
        let (client, mut server) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            server.write_all(b"GET / HTTP/1.1\r\nHost: test\r\n\r\nbody").await.unwrap();
            server.shutdown().await.unwrap();
        });
        Ok(client)
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
    provided: Arc<AtomicUsize>,
    written: Arc<AtomicUsize>,
    raw: Arc<AtomicUsize>,
}
impl CountingDestination {
    fn new() -> (Self, Arc<AtomicUsize>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let p = Arc::new(AtomicUsize::new(0));
        let w = Arc::new(AtomicUsize::new(0));
        let r = Arc::new(AtomicUsize::new(0));
        (Self { provided: p.clone(), written: w.clone(), raw: r.clone() }, p, w, r)
    }
}
impl StreamDestination for CountingDestination {
    type Frame = TestFrame;
    type Error = std::io::Error;
    async fn provide(&mut self, _frame: &Self::Frame) -> Result<(), Self::Error> {
        self.provided.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn write(&mut self, _frame: Self::Frame) -> Result<(), Self::Error> {
        self.written.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn write_raw(&mut self, _bytes: Bytes) -> Result<(), Self::Error> {
        self.raw.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn pipeline_drives_connection() {
    let source = OneShotSource;
    let stream = source.accept().await.unwrap();
    let mut parser = MockParser;
    let pipeline = NoopPipeline;
    let (mut dest, provided, written, raw) = CountingDestination::new();

    let frames = parser.parse(stream);
    let mut frames = pin!(frames);
    let mut routed = false;

    while let Some(Ok(item)) = frames.next().await {
        match item {
            ParsedData::Passthrough(bytes) => {
                dest.write_raw(bytes).await.unwrap();
            }
            ParsedData::Parsed(frame) => {
                if frame.is_routing_frame() && !routed {
                    dest.provide(&frame).await.unwrap();
                    routed = true;
                }
                let frame = pipeline.process(frame);
                dest.write(frame).await.unwrap();
            }
        }
    }

    assert_eq!(provided.load(Ordering::SeqCst), 1);
    assert_eq!(written.load(Ordering::SeqCst), 2);
    assert_eq!(raw.load(Ordering::SeqCst), 1);
}
