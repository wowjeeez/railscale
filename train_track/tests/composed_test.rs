use std::io;
use bytes::Bytes;
use tokio::io::AsyncReadExt;
use tokio_stream::StreamExt;
use train_track::{Frame, ParsedData, FrameParser, RailscaleError, StreamTransform, Composed};

struct LineFrame(Bytes);

impl Frame for LineFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct LineParser;

impl FrameParser<io::Cursor<Vec<u8>>> for LineParser {
    type Frame = LineFrame;
    type Error = RailscaleError;

    fn parse(&mut self, mut stream: io::Cursor<Vec<u8>>) -> impl tokio_stream::Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        async_stream::stream! {
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            for line in buf.split(|&b| b == b'\n') {
                if !line.is_empty() {
                    yield Ok(ParsedData::Parsed(LineFrame(Bytes::copy_from_slice(line))));
                }
            }
        }
    }
}

struct PassthroughParser;

impl FrameParser<io::Cursor<Vec<u8>>> for PassthroughParser {
    type Frame = LineFrame;
    type Error = RailscaleError;

    fn parse(&mut self, mut stream: io::Cursor<Vec<u8>>) -> impl tokio_stream::Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        async_stream::stream! {
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            if !buf.is_empty() {
                yield Ok(ParsedData::Passthrough(Bytes::from(buf)));
            }
        }
    }
}

struct UppercaseTransform;

#[async_trait::async_trait]
impl StreamTransform for UppercaseTransform {
    type Input = io::Cursor<Vec<u8>>;
    type Output = io::Cursor<Vec<u8>>;
    type Error = RailscaleError;

    async fn transform(self, mut stream: Self::Input) -> Result<Self::Output, Self::Error> {
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await?;
        buf.make_ascii_uppercase();
        Ok(io::Cursor::new(buf))
    }
}

struct FailTransform;

#[async_trait::async_trait]
impl StreamTransform for FailTransform {
    type Input = io::Cursor<Vec<u8>>;
    type Output = io::Cursor<Vec<u8>>;
    type Error = RailscaleError;

    async fn transform(self, _stream: Self::Input) -> Result<Self::Output, Self::Error> {
        Err(io::Error::new(io::ErrorKind::Other, "transform failed").into())
    }
}

struct IdentityTransform;

#[async_trait::async_trait]
impl StreamTransform for IdentityTransform {
    type Input = io::Cursor<Vec<u8>>;
    type Output = io::Cursor<Vec<u8>>;
    type Error = RailscaleError;

    async fn transform(self, mut stream: Self::Input) -> Result<Self::Output, Self::Error> {
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await?;
        Ok(io::Cursor::new(buf))
    }
}

#[tokio::test]
async fn composed_transforms_then_parses() {
    let mut composed = Composed::new(UppercaseTransform, || LineParser);
    let input = io::Cursor::new(b"hello\nworld\n".to_vec());
    let stream = composed.parse(input);
    tokio::pin!(stream);

    let mut frames = Vec::new();
    while let Some(Ok(ParsedData::Parsed(f))) = stream.next().await {
        frames.push(String::from_utf8(f.into_bytes().to_vec()).unwrap());
    }

    assert_eq!(frames, vec!["HELLO", "WORLD"]);
}

#[tokio::test]
async fn composed_propagates_transform_error() {
    let mut composed = Composed::new(FailTransform, || LineParser);
    let input = io::Cursor::new(b"data".to_vec());
    let stream = composed.parse(input);
    tokio::pin!(stream);

    let item = stream.next().await.unwrap();
    assert!(item.is_err());
}

#[tokio::test]
async fn composed_passthrough_bytes_forwarded() {
    let mut composed = Composed::new(IdentityTransform, || PassthroughParser);
    let input = io::Cursor::new(b"raw bytes here".to_vec());
    let stream = composed.parse(input);
    tokio::pin!(stream);

    let item = stream.next().await.unwrap().unwrap();
    match item {
        ParsedData::Passthrough(b) => assert_eq!(&b[..], b"raw bytes here"),
        _ => panic!("expected Passthrough"),
    }
}
