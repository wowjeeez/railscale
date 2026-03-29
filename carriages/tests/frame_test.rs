use bytes::Bytes;
use train_track::Frame;
use locomotive::HttpFrame;

#[test]
fn header_frame_is_routing_when_request_line() {
    let f = HttpFrame::header(Bytes::from_static(b"GET / HTTP/1.1"), true);
    assert!(f.is_routing_frame());
    assert_eq!(f.as_bytes(), b"GET / HTTP/1.1");
}

#[test]
fn header_frame_not_routing_for_normal_headers() {
    let f = HttpFrame::header(Bytes::from_static(b"Host: example.com"), false);
    assert!(!f.is_routing_frame());
}

#[test]
fn into_bytes_returns_data() {
    let f = HttpFrame::header(Bytes::from_static(b"Content-Type: text/plain"), false);
    let b = f.into_bytes();
    assert_eq!(&b[..], b"Content-Type: text/plain");
}
