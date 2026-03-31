use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::ReadBuf;
use tokio_stream::StreamExt;
use train_track::{FrameParser, ParsedData, Frame};
use trezorcarriage::{TlsParser, TlsRecordType};

fn make_tls_record(record_type: u8, payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u16;
    let mut record = vec![record_type, 0x03, 0x03, (len >> 8) as u8, len as u8];
    record.extend_from_slice(payload);
    record
}

struct SlowReader {
    data: Vec<u8>,
    pos: usize,
    chunk_size: usize,
}

impl SlowReader {
    fn new(data: Vec<u8>, chunk_size: usize) -> Self {
        Self { data, pos: 0, chunk_size }
    }
}

impl tokio::io::AsyncRead for SlowReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let remaining = &self.data[self.pos..];
        if remaining.is_empty() {
            return Poll::Ready(Ok(()));
        }
        let to_read = remaining.len().min(self.chunk_size).min(buf.remaining());
        buf.put_slice(&remaining[..to_read]);
        self.pos += to_read;
        Poll::Ready(Ok(()))
    }
}

impl Unpin for SlowReader {}

#[tokio::test]
async fn parses_single_handshake_record() {
    let payload = vec![0xAAu8; 10];
    let data = make_tls_record(22, &payload);
    let cursor = io::Cursor::new(data);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, cursor));
    let item = stream.next().await.unwrap().unwrap();
    match item {
        ParsedData::Parsed(frame) => {
            assert_eq!(frame.record_type(), TlsRecordType::Handshake);
            let expected = make_tls_record(22, &payload);
            assert_eq!(frame.as_bytes(), expected.as_slice());
        }
        _ => panic!("expected Parsed"),
    }
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn parses_multiple_records() {
    let p1 = vec![1u8; 5];
    let p2 = vec![2u8; 10];
    let p3 = vec![3u8; 3];
    let mut data = make_tls_record(22, &p1);
    data.extend(make_tls_record(23, &p2));
    data.extend(make_tls_record(21, &p3));
    let cursor = io::Cursor::new(data);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, cursor));

    let item1 = stream.next().await.unwrap().unwrap();
    match item1 {
        ParsedData::Parsed(f) => {
            assert_eq!(f.record_type(), TlsRecordType::Handshake);
            assert_eq!(f.as_bytes(), make_tls_record(22, &p1).as_slice());
        }
        _ => panic!("expected Parsed"),
    }

    let item2 = stream.next().await.unwrap().unwrap();
    match item2 {
        ParsedData::Parsed(f) => {
            assert_eq!(f.record_type(), TlsRecordType::ApplicationData);
            assert_eq!(f.as_bytes(), make_tls_record(23, &p2).as_slice());
        }
        _ => panic!("expected Parsed"),
    }

    let item3 = stream.next().await.unwrap().unwrap();
    match item3 {
        ParsedData::Parsed(f) => {
            assert_eq!(f.record_type(), TlsRecordType::Alert);
            assert_eq!(f.as_bytes(), make_tls_record(21, &p3).as_slice());
        }
        _ => panic!("expected Parsed"),
    }

    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn handles_record_split_across_reads() {
    let payload = vec![0xBBu8; 20];
    let data = make_tls_record(22, &payload);
    let reader = SlowReader::new(data, 1);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, reader));
    let item = stream.next().await.unwrap().unwrap();
    match item {
        ParsedData::Parsed(f) => {
            assert_eq!(f.record_type(), TlsRecordType::Handshake);
            let expected = make_tls_record(22, &payload);
            assert_eq!(f.as_bytes(), expected.as_slice());
        }
        _ => panic!("expected Parsed"),
    }
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn rejects_unknown_record_type() {
    let payload = vec![0u8; 5];
    let data = make_tls_record(99, &payload);
    let cursor = io::Cursor::new(data);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, cursor));
    let result = stream.next().await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn empty_stream_yields_nothing() {
    let cursor = io::Cursor::new(vec![]);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, cursor));
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn record_with_max_payload_16384() {
    let payload = vec![0xCCu8; 16384];
    let data = make_tls_record(23, &payload);
    let cursor = io::Cursor::new(data);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, cursor));
    let item = stream.next().await.unwrap().unwrap();
    match item {
        ParsedData::Parsed(f) => {
            assert_eq!(f.record_type(), TlsRecordType::ApplicationData);
            assert_eq!(f.as_bytes().len(), 16384 + 5);
        }
        _ => panic!("expected Parsed"),
    }
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn rejects_oversized_record() {
    let len: u16 = 16385;
    let header = vec![22u8, 0x03, 0x03, (len >> 8) as u8, len as u8];
    let cursor = io::Cursor::new(header);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, cursor));
    let result = stream.next().await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn incomplete_header_yields_error() {
    let data = vec![22u8, 0x03, 0x03];
    let cursor = io::Cursor::new(data);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, cursor));
    let result = stream.next().await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn incomplete_payload_yields_error() {
    let mut data = vec![22u8, 0x03, 0x03, 0x00, 100u8];
    data.extend(vec![0xAAu8; 50]);
    let cursor = io::Cursor::new(data);
    let mut parser = TlsParser::new();
    let mut stream = Box::pin(FrameParser::parse(&mut parser, cursor));
    let result = stream.next().await.unwrap();
    assert!(result.is_err());
}
