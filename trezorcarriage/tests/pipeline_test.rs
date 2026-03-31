use bytes::Bytes;
use train_track::{Frame, FramePipeline};
use trezorcarriage::{TlsEncryptedFrame, TlsRecordType, TlsPassthroughPipeline, Passthrough};

#[test]
fn passthrough_returns_frame_unchanged() {
    let pipeline = TlsPassthroughPipeline::<Passthrough>::new();
    let data = Bytes::from_static(b"encrypted payload");
    let frame = TlsEncryptedFrame::new(data.clone(), TlsRecordType::ApplicationData);
    let result = pipeline.process(frame);
    assert_eq!(result.as_bytes(), &data[..]);
    assert_eq!(result.record_type(), TlsRecordType::ApplicationData);
}

#[test]
fn passthrough_preserves_handshake() {
    let pipeline = TlsPassthroughPipeline::<Passthrough>::new();
    let data = Bytes::from_static(b"\x01\x00\x00\x05hello");
    let frame = TlsEncryptedFrame::new(data.clone(), TlsRecordType::Handshake);
    let result = pipeline.process(frame);
    assert_eq!(result.as_bytes(), &data[..]);
    assert_eq!(result.record_type(), TlsRecordType::Handshake);
}

#[test]
fn passthrough_preserves_alert() {
    let pipeline = TlsPassthroughPipeline::<Passthrough>::new();
    let frame = TlsEncryptedFrame::new(Bytes::from_static(b"\x02\x00"), TlsRecordType::Alert);
    let result = pipeline.process(frame);
    assert_eq!(result.record_type(), TlsRecordType::Alert);
}

#[test]
fn passthrough_preserves_change_cipher_spec() {
    let pipeline = TlsPassthroughPipeline::<Passthrough>::new();
    let frame = TlsEncryptedFrame::new(Bytes::from_static(b"\x01"), TlsRecordType::ChangeCipherSpec);
    let result = pipeline.process(frame);
    assert_eq!(result.record_type(), TlsRecordType::ChangeCipherSpec);
}
