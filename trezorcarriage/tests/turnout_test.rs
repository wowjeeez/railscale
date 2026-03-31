use bytes::Bytes;
use train_track::{Frame, Turnout, SwitchRail};
use trezorcarriage::{TlsEncryptedFrame, TlsRecordType, TlsPassthroughTurnout, TlsTerminationRail};

#[test]
fn tls_passthrough_turnout_passes_frame_through() {
    let turnout = TlsPassthroughTurnout::new();
    let frame = TlsEncryptedFrame::new(
        Bytes::from_static(b"\x17\x03\x03\x00\x05hello"),
        TlsRecordType::ApplicationData,
    );
    let output = turnout.process(frame).unwrap();
    assert_eq!(output.as_bytes(), b"\x17\x03\x03\x00\x05hello");
}

#[test]
fn tls_passthrough_turnout_never_drops() {
    let turnout = TlsPassthroughTurnout::new();
    let frame = TlsEncryptedFrame::new(
        Bytes::from_static(b"\x16\x03\x03\x00\x05hello"),
        TlsRecordType::Handshake,
    );
    assert!(turnout.process(frame).is_some());
}

#[test]
fn tls_passthrough_turnout_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TlsPassthroughTurnout>();
}

#[test]
fn tls_termination_rail_converts_to_http_frame() {
    let rail = TlsTerminationRail;
    let tls_frame = TlsEncryptedFrame::new(
        Bytes::from_static(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n"),
        TlsRecordType::ApplicationData,
    );
    let http_frame = rail.switch(tls_frame);
    assert_eq!(http_frame.as_bytes(), b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n");
}

#[test]
fn tls_termination_rail_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TlsTerminationRail>();
}
