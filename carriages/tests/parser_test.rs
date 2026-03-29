use bytes::Bytes;
use std::pin::pin;
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;
use train_track::{Frame, FrameParser, ParsedData};
use locomotive::HttpParser;
use memchr::memmem::Finder;

#[tokio::test]
async fn parses_http_request_into_frames_and_passthrough() {
    let (client, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        server.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\nbody data here").await.unwrap();
        server.shutdown().await.unwrap();
    });

    let mut parser = HttpParser::new(vec![]);
    let stream = parser.parse(client);
    let mut stream = pin!(stream);

    let mut items: Vec<String> = vec![];
    let mut passthrough_bytes = 0usize;
    let mut had_routing = false;

    while let Some(Ok(item)) = stream.next().await {
        match item {
            ParsedData::Parsed(frame) => {
                if frame.is_routing_frame() {
                    had_routing = true;
                }
                items.push(String::from_utf8_lossy(frame.as_bytes()).into_owned());
            }
            ParsedData::Passthrough(bytes) => {
                passthrough_bytes += bytes.len();
            }
        }
    }

    assert!(had_routing);
    assert_eq!(items[0], "GET / HTTP/1.1");
    assert_eq!(items[1], "Host: example.com");
    assert!(passthrough_bytes > 0);
}

#[tokio::test]
async fn applies_matchers_during_parse() {
    let (client, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        server.write_all(b"GET /\r\nHost: original.com\r\n\r\n").await.unwrap();
        server.shutdown().await.unwrap();
    });

    let matchers = vec![
        (Finder::new(b"Host"), Bytes::from_static(b"replaced.com")),
    ];
    let mut parser = HttpParser::new(matchers);
    let stream = parser.parse(client);
    let mut stream = pin!(stream);

    let mut headers: Vec<String> = vec![];
    while let Some(Ok(item)) = stream.next().await {
        match item {
            ParsedData::Parsed(frame) => {
                headers.push(String::from_utf8_lossy(frame.as_bytes()).into_owned());
            }
            _ => {}
        }
    }

    assert_eq!(headers[1], "Host: replaced.com");
}
