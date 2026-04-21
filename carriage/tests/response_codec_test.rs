use bytes::BytesMut;
use tokio_util::codec::Decoder;
use carriage::http_v1::HttpPhase;
use carriage::http_v1::derive::BodyFramingMode;
use train_track::{Frame, PhasedFrame};

fn codec() -> carriage::http_v1::response_codec::ResponseCodec {
    carriage::http_v1::response_codec::ResponseCodec::new()
}

fn item_frame(item: &carriage::http_v1::response_codec::ResponseCodecItem) -> &carriage::http_v1::HttpFrame {
    match item {
        carriage::http_v1::response_codec::ResponseCodecItem::Frame(f) => f,
        _ => panic!("expected Frame"),
    }
}

fn item_body(item: &carriage::http_v1::response_codec::ResponseCodecItem) -> &[u8] {
    match item {
        carriage::http_v1::response_codec::ResponseCodecItem::Body(b) => b,
        _ => panic!("expected Body"),
    }
}

#[test]
fn status_line_parsed() {
    let mut c = codec();
    let mut buf = BytesMut::from("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi");
    let item = c.decode(&mut buf).unwrap().unwrap();
    let frame = item_frame(&item);
    assert_eq!(frame.phase(), HttpPhase::StatusLine);
    assert_eq!(frame.as_bytes(), b"HTTP/1.1 200 OK\r\n");
}

#[test]
fn content_length_body_framing() {
    let mut c = codec();
    let mut buf = BytesMut::from("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");

    c.decode(&mut buf).unwrap();
    c.decode(&mut buf).unwrap();
    c.decode(&mut buf).unwrap();

    assert_eq!(c.body_framing_mode(), Some(BodyFramingMode::Fixed(5)));

    let body = c.decode(&mut buf).unwrap().unwrap();
    assert_eq!(item_body(&body), b"hello");
    assert!(c.is_response_complete());
}

#[test]
fn chunked_body_framing() {
    let mut c = codec();
    let mut buf = BytesMut::from(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n"
    );

    c.decode(&mut buf).unwrap();
    c.decode(&mut buf).unwrap();
    c.decode(&mut buf).unwrap();

    assert_eq!(c.body_framing_mode(), Some(BodyFramingMode::Chunked));

    let item = c.decode(&mut buf).unwrap().unwrap();
    assert_eq!(item_body(&item), b"5\r\n");

    let item = c.decode(&mut buf).unwrap().unwrap();
    assert_eq!(item_body(&item), b"hello");

    let item = c.decode(&mut buf).unwrap().unwrap();
    assert_eq!(item_body(&item), b"\r\n");

    let item = c.decode(&mut buf).unwrap().unwrap();
    assert_eq!(item_body(&item), b"0\r\n");

    let item = c.decode(&mut buf).unwrap().unwrap();
    assert_eq!(item_body(&item), b"\r\n");

    assert!(c.is_response_complete());
}

#[test]
fn eof_body_framing_http10() {
    let mut c = codec();
    let mut buf = BytesMut::from("HTTP/1.0 200 OK\r\n\r\nsome body");

    c.decode(&mut buf).unwrap();
    c.decode(&mut buf).unwrap();

    assert_eq!(c.body_framing_mode(), Some(BodyFramingMode::UntilClose));

    let item = c.decode(&mut buf).unwrap().unwrap();
    assert_eq!(item_body(&item), b"some body");
    assert!(!c.is_response_complete());

    let mut empty = BytesMut::new();
    let result = c.decode_eof(&mut empty).unwrap();
    assert!(result.is_none());
    assert!(c.is_response_complete());
}

#[test]
fn obs_fold_rejected_in_response() {
    let mut c = codec();
    let mut buf = BytesMut::from("HTTP/1.1 200 OK\r\n continuation: bad\r\n\r\n");
    c.decode(&mut buf).unwrap();
    let result = c.decode(&mut buf);
    assert!(result.is_err());
}

#[test]
fn zero_content_length_completes_immediately() {
    let mut c = codec();
    let mut buf = BytesMut::from("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");

    c.decode(&mut buf).unwrap();
    c.decode(&mut buf).unwrap();
    c.decode(&mut buf).unwrap();

    assert!(c.is_response_complete());
}

#[test]
fn no_body_headers_completes_for_http11() {
    let mut c = codec();
    let mut buf = BytesMut::from("HTTP/1.1 200 OK\r\n\r\n");

    c.decode(&mut buf).unwrap();
    c.decode(&mut buf).unwrap();

    assert_eq!(c.body_framing_mode(), Some(BodyFramingMode::None));
    assert!(c.is_response_complete());
}
