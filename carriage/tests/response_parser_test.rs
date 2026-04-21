use std::pin::pin;
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;
use train_track::{FrameParser, ParsedData, PhasedFrame};
use carriage::http_v1::{HttpPhase, ResponseParser};

#[tokio::test]
async fn parse_simple_response() {
    let (client, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        server.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello").await.unwrap();
        server.shutdown().await.unwrap();
    });

    let mut parser = ResponseParser::new();
    let mut stream = pin!(parser.parse(client));

    let item = stream.next().await.unwrap().unwrap();
    match item {
        ParsedData::Parsed(frame) => {
            assert_eq!(frame.phase(), HttpPhase::StatusLine);
        }
        _ => panic!("expected status line frame"),
    }

    let item = stream.next().await.unwrap().unwrap();
    match item {
        ParsedData::Parsed(frame) => {
            assert_eq!(frame.phase(), HttpPhase::Header);
        }
        _ => panic!("expected header frame"),
    }

    let item = stream.next().await.unwrap().unwrap();
    match item {
        ParsedData::Parsed(frame) => {
            assert_eq!(frame.phase(), HttpPhase::EndOfHeaders);
        }
        _ => panic!("expected end of headers frame"),
    }

    let item = stream.next().await.unwrap().unwrap();
    match item {
        ParsedData::Passthrough(bytes) => {
            assert_eq!(&bytes[..], b"hello");
        }
        _ => panic!("expected passthrough body"),
    }

    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn parse_chunked_response() {
    let (client, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        server.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n").await.unwrap();
        server.shutdown().await.unwrap();
    });

    let mut parser = ResponseParser::new();
    let stream = pin!(parser.parse(client));

    let items: Vec<_> = stream.collect().await;
    assert!(items.iter().all(|r| r.is_ok()));

    let has_status = items.iter().any(|r| {
        matches!(r.as_ref().unwrap(), ParsedData::Parsed(f) if f.phase() == HttpPhase::StatusLine)
    });
    assert!(has_status);

    let body_bytes: Vec<u8> = items.iter()
        .filter_map(|r| match r.as_ref().unwrap() {
            ParsedData::Passthrough(b) => Some(b.to_vec()),
            _ => None,
        })
        .flatten()
        .collect();
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(body_str.contains("abc"));
}
