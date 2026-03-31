use bytes::Bytes;
use memchr::memmem::Finder;
use train_track::{Frame, Turnout};
use carriage::http_v1::HttpFrame;
use carriage::HttpTurnout;

#[test]
fn http_turnout_passthrough_returns_frame_unchanged() {
    let turnout = HttpTurnout::passthrough();
    let frame = HttpFrame::header(Bytes::from_static(b"Host: example.com\r\n"));
    let output = turnout.process(frame).unwrap();
    assert_eq!(output.as_bytes(), b"Host: example.com\r\n");
}

#[test]
fn http_turnout_with_replacements_replaces_header() {
    let turnout = HttpTurnout::new(vec![
        (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
    ]);
    let frame = HttpFrame::header(Bytes::from_static(b"User-Agent: curl/7.0\r\n"));
    let output = turnout.process(frame).unwrap();
    assert_eq!(output.as_bytes(), b"User-Agent: railscale/1.0\r\n");
}

#[test]
fn http_turnout_no_match_passes_through() {
    let turnout = HttpTurnout::new(vec![
        (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
    ]);
    let frame = HttpFrame::header(Bytes::from_static(b"Host: example.com\r\n"));
    let output = turnout.process(frame).unwrap();
    assert_eq!(output.as_bytes(), b"Host: example.com\r\n");
}

#[test]
fn http_turnout_never_drops_frames() {
    let turnout = HttpTurnout::passthrough();
    let frame = HttpFrame::header(Bytes::from_static(b"X-Custom: value\r\n"));
    assert!(turnout.process(frame).is_some());
}

#[test]
fn http_turnout_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<HttpTurnout>();
}
