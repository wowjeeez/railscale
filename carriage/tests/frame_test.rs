use bytes::Bytes;
use train_track::{Frame, PhasedFrame};
use carriage::{HttpFrame, HttpPhase};

#[test]
fn request_line_is_routing() {
    let f = HttpFrame::request_line(Bytes::from_static(b"GET / HTTP/1.1"));
    assert!(f.routing_key().is_some());
    assert_eq!(f.as_bytes(), b"GET / HTTP/1.1");
    assert_eq!(f.phase(), HttpPhase::RequestLine);
}

#[test]
fn header_frame_not_routing() {
    let f = HttpFrame::header(Bytes::from_static(b"Host: example.com"));
    assert!(f.routing_key().is_none());
    assert_eq!(f.phase(), HttpPhase::Header);
}

#[test]
fn into_bytes_returns_data() {
    let f = HttpFrame::header(Bytes::from_static(b"Content-Type: text/plain"));
    let b = f.into_bytes();
    assert_eq!(&b[..], b"Content-Type: text/plain");
}

#[test]
fn end_of_headers_phase() {
    let f = HttpFrame::end_of_headers();
    assert!(f.is_end_of_headers());
    assert_eq!(f.phase(), HttpPhase::EndOfHeaders);
}

#[test]
fn body_phase() {
    let f = HttpFrame::body(Bytes::from_static(b"hello"));
    assert_eq!(f.phase(), HttpPhase::Body);
    assert!(f.routing_key().is_none());
}

#[test]
fn phase_ordering() {
    assert!(HttpPhase::RequestLine < HttpPhase::Header);
    assert!(HttpPhase::Header < HttpPhase::EndOfHeaders);
    assert!(HttpPhase::EndOfHeaders < HttpPhase::Body);
    assert!(HttpPhase::Body < HttpPhase::Trailer);
}
