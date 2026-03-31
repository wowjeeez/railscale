use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;
use train_track::{Frame, FrameParser, ParsedData, RawFrame, RawParser};

#[test]
fn raw_frame_as_bytes() {
    let frame = RawFrame::new(Bytes::from_static(b"hello"));
    assert_eq!(frame.as_bytes(), b"hello");
}

#[test]
fn raw_frame_into_bytes() {
    let frame = RawFrame::new(Bytes::from_static(b"hello"));
    assert_eq!(frame.into_bytes(), Bytes::from_static(b"hello"));
}

#[test]
fn raw_frame_routing_key_is_none() {
    let frame = RawFrame::new(Bytes::from_static(b"hello"));
    assert!(frame.routing_key().is_none());
}

#[tokio::test]
async fn raw_parser_yields_chunks() {
    let data = b"hello world this is a test";
    let (mut writer, reader) = tokio::io::duplex(1024);
    writer.write_all(data).await.unwrap();
    drop(writer);

    let mut parser = RawParser::new();
    let stream = parser.parse(reader);
    tokio::pin!(stream);

    let mut total = Vec::new();
    while let Some(Ok(parsed)) = stream.next().await {
        match parsed {
            ParsedData::Parsed(frame) => total.extend_from_slice(frame.as_bytes()),
            ParsedData::Passthrough(bytes) => total.extend_from_slice(&bytes),
        }
    }
    assert_eq!(total, data);
}

#[tokio::test]
async fn raw_parser_empty_stream_yields_nothing() {
    let (writer, reader) = tokio::io::duplex(1024);
    drop(writer);

    let mut parser = RawParser::new();
    let stream = parser.parse(reader);
    tokio::pin!(stream);

    let mut count = 0;
    while let Some(Ok(_)) = stream.next().await {
        count += 1;
    }
    assert_eq!(count, 0);
}
