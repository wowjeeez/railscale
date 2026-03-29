use bytes::BytesMut;
use tokio_util::codec::Decoder;
use locomotive::{HttpStreamingCodec, HttpFrame};
use train_track::Frame;

#[test]
fn decodes_request_line() {
    let mut codec = HttpStreamingCodec::new(vec![]);
    let mut buf = BytesMut::from("GET / HTTP/1.1\r\n");
    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert!(frame.is_routing_frame());
    assert_eq!(frame.as_bytes(), b"GET / HTTP/1.1");
}

#[test]
fn decodes_header_line() {
    let mut codec = HttpStreamingCodec::new(vec![]);
    let mut buf = BytesMut::from("GET /\r\nHost: example.com\r\n");
    let _ = codec.decode(&mut buf).unwrap();
    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert!(!frame.is_routing_frame());
    assert_eq!(frame.as_bytes(), b"Host: example.com");
}

#[test]
fn detects_end_of_headers() {
    let mut codec = HttpStreamingCodec::new(vec![]);
    let mut buf = BytesMut::from("GET /\r\n\r\n");
    let _ = codec.decode(&mut buf).unwrap();
    let end = codec.decode(&mut buf).unwrap().unwrap();
    assert!(end.is_end_of_headers());
    assert!(codec.headers_done());
}

#[test]
fn applies_matcher_replacement() {
    use bytes::Bytes;
    use memchr::memmem::Finder;
    let matchers = vec![
        (Finder::new(b"Host"), Bytes::from_static(b"rewritten.local")),
    ];
    let mut codec = HttpStreamingCodec::new(matchers);
    let mut buf = BytesMut::from("GET /\r\nHost: example.com\r\n");
    let _ = codec.decode(&mut buf).unwrap();
    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(frame.as_bytes(), b"Host: rewritten.local");
}

#[test]
fn waits_for_complete_line() {
    let mut codec = HttpStreamingCodec::new(vec![]);
    let mut buf = BytesMut::from("GET / HTTP/1.1");
    let result = codec.decode(&mut buf).unwrap();
    assert!(result.is_none());
}
