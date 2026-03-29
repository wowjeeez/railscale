use bytes::Bytes;
use tokio::io::AsyncRead;
use tokio_stream::{Stream, StreamExt};
use train_track::{Frame, FrameParser, ParsedData, RailscaleError};

struct SimpleFrame(Bytes);

impl Frame for SimpleFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn is_routing_frame(&self) -> bool { false }
}

struct LineParser;

impl<S: AsyncRead + Send + Unpin> FrameParser<S> for LineParser {
    type Frame = SimpleFrame;
    type Error = std::io::Error;

    fn parse(&mut self, _stream: S) -> impl Stream<Item = Result<ParsedData<Self::Frame>, Self::Error>> + Send {
        tokio_stream::iter(vec![
            Ok(ParsedData::Parsed(SimpleFrame(Bytes::from_static(b"line1")))),
            Ok(ParsedData::Passthrough(Bytes::from_static(b"body"))),
        ])
    }
}

#[tokio::test]
async fn parser_yields_parsed_and_passthrough() {
    let mut parser = LineParser;
    let (client, _server) = tokio::io::duplex(64);
    let mut stream = std::pin::pin!(parser.parse(client));

    let first = stream.next().await.unwrap().unwrap();
    assert!(matches!(first, ParsedData::Parsed(_)));

    let second = stream.next().await.unwrap().unwrap();
    assert!(matches!(second, ParsedData::Passthrough(_)));

    assert!(stream.next().await.is_none());
}
