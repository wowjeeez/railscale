use bytes::Bytes;
use trezorcarriage::{TlsEncryptedFrame, TlsRecordType};
use train_track::Frame;

fn build_client_hello_with_sni(hostname: &[u8]) -> Vec<u8> {
    let name_len = hostname.len();
    let server_name_list_len = 1 + 2 + name_len;
    let sni_data_len = 2 + server_name_list_len;
    let _extensions_data_len = 2 + 2 + sni_data_len;

    let client_hello_body_len =
        2  // version
        + 32 // random
        + 1  // session id length
        + 2  // cipher suites length
        + 2  // cipher suite
        + 1  // compression methods length
        + 1  // compression method
        + 2  // extensions length
        + 2  // extension type
        + 2  // extension data length
        + 2  // server name list length
        + 1  // name type
        + 2  // name length
        + name_len;

    let mut buf = Vec::new();
    buf.push(0x01);
    buf.push(((client_hello_body_len >> 16) & 0xff) as u8);
    buf.push(((client_hello_body_len >> 8) & 0xff) as u8);
    buf.push((client_hello_body_len & 0xff) as u8);

    buf.extend_from_slice(&[0x03, 0x03]);
    buf.extend_from_slice(&[0u8; 32]);
    buf.push(0x00);
    buf.extend_from_slice(&[0x00, 0x02]);
    buf.extend_from_slice(&[0x00, 0x9c]);
    buf.push(0x01);
    buf.push(0x00);

    let ext_total_len = 2 + 2 + sni_data_len as u16;
    buf.push(((ext_total_len >> 8) & 0xff) as u8);
    buf.push((ext_total_len & 0xff) as u8);

    buf.extend_from_slice(&[0x00, 0x00]);
    buf.push(((sni_data_len >> 8) & 0xff) as u8);
    buf.push((sni_data_len & 0xff) as u8);

    buf.push(((server_name_list_len >> 8) & 0xff) as u8);
    buf.push((server_name_list_len & 0xff) as u8);

    buf.push(0x00);
    buf.push(((name_len >> 8) & 0xff) as u8);
    buf.push((name_len & 0xff) as u8);
    buf.extend_from_slice(hostname);

    buf
}

fn build_client_hello_with_extra_extension_before_sni(hostname: &[u8]) -> Vec<u8> {
    let name_len = hostname.len();
    let server_name_list_len = 1 + 2 + name_len;
    let sni_data_len = 2 + server_name_list_len;

    let dummy_ext_data: &[u8] = &[0xde, 0xad];
    let dummy_ext_len = dummy_ext_data.len();

    let ext_total_len =
        2 + 2 + dummy_ext_len   // dummy extension
        + 2 + 2 + sni_data_len; // SNI extension

    let client_hello_body_len =
        2 + 32 + 1 + 2 + 2 + 1 + 1 + 2 + ext_total_len;

    let mut buf = Vec::new();
    buf.push(0x01);
    buf.push(((client_hello_body_len >> 16) & 0xff) as u8);
    buf.push(((client_hello_body_len >> 8) & 0xff) as u8);
    buf.push((client_hello_body_len & 0xff) as u8);

    buf.extend_from_slice(&[0x03, 0x03]);
    buf.extend_from_slice(&[0u8; 32]);
    buf.push(0x00);
    buf.extend_from_slice(&[0x00, 0x02]);
    buf.extend_from_slice(&[0x00, 0x9c]);
    buf.push(0x01);
    buf.push(0x00);

    buf.push(((ext_total_len >> 8) & 0xff) as u8);
    buf.push((ext_total_len & 0xff) as u8);

    buf.extend_from_slice(&[0xff, 0x01]);
    buf.push(((dummy_ext_len >> 8) & 0xff) as u8);
    buf.push((dummy_ext_len & 0xff) as u8);
    buf.extend_from_slice(dummy_ext_data);

    buf.extend_from_slice(&[0x00, 0x00]);
    buf.push(((sni_data_len >> 8) & 0xff) as u8);
    buf.push((sni_data_len & 0xff) as u8);

    buf.push(((server_name_list_len >> 8) & 0xff) as u8);
    buf.push((server_name_list_len & 0xff) as u8);

    buf.push(0x00);
    buf.push(((name_len >> 8) & 0xff) as u8);
    buf.push((name_len & 0xff) as u8);
    buf.extend_from_slice(hostname);

    buf
}

#[test]
fn frame_as_bytes_returns_data() {
    let data = Bytes::from_static(b"hello");
    let frame = TlsEncryptedFrame::new(data.clone(), TlsRecordType::ApplicationData);
    assert_eq!(frame.as_bytes(), b"hello");
}

#[test]
fn frame_into_bytes_consumes() {
    let data = Bytes::from_static(b"world");
    let frame = TlsEncryptedFrame::new(data.clone(), TlsRecordType::ApplicationData);
    assert_eq!(frame.into_bytes(), data);
}

#[test]
fn frame_record_type() {
    let frame = TlsEncryptedFrame::new(Bytes::new(), TlsRecordType::Alert);
    assert_eq!(frame.record_type(), TlsRecordType::Alert);
}

#[test]
fn handshake_frame_extracts_sni_routing_key() {
    let payload = build_client_hello_with_sni(b"example.com");
    let frame = TlsEncryptedFrame::new(Bytes::from(payload), TlsRecordType::Handshake);
    assert_eq!(frame.routing_key(), Some(b"example.com".as_ref()));
}

#[test]
fn non_handshake_frame_has_no_routing_key() {
    let payload = build_client_hello_with_sni(b"example.com");
    let frame = TlsEncryptedFrame::new(Bytes::from(payload), TlsRecordType::ApplicationData);
    assert_eq!(frame.routing_key(), None);
}

#[test]
fn handshake_without_sni_has_no_routing_key() {
    let client_hello_body_len: usize = 2 + 32 + 1 + 2 + 2 + 1 + 1;
    let mut buf = Vec::new();
    buf.push(0x02);
    buf.push(((client_hello_body_len >> 16) & 0xff) as u8);
    buf.push(((client_hello_body_len >> 8) & 0xff) as u8);
    buf.push((client_hello_body_len & 0xff) as u8);
    buf.extend_from_slice(&[0x03, 0x03]);
    buf.extend_from_slice(&[0u8; 32]);
    buf.push(0x00);
    buf.extend_from_slice(&[0x00, 0x02]);
    buf.extend_from_slice(&[0x00, 0x9c]);
    buf.push(0x01);
    buf.push(0x00);

    let frame = TlsEncryptedFrame::new(Bytes::from(buf), TlsRecordType::Handshake);
    assert_eq!(frame.routing_key(), None);
}

#[test]
fn record_type_from_u8() {
    assert_eq!(TlsRecordType::from_u8(20), Some(TlsRecordType::ChangeCipherSpec));
    assert_eq!(TlsRecordType::from_u8(21), Some(TlsRecordType::Alert));
    assert_eq!(TlsRecordType::from_u8(22), Some(TlsRecordType::Handshake));
    assert_eq!(TlsRecordType::from_u8(23), Some(TlsRecordType::ApplicationData));
    assert_eq!(TlsRecordType::from_u8(99), None);
}

#[test]
fn record_type_as_u8_roundtrips() {
    for val in [20u8, 21, 22, 23] {
        let rt = TlsRecordType::from_u8(val).unwrap();
        assert_eq!(rt.as_u8(), val);
    }
}

#[test]
fn sni_with_multiple_extensions() {
    let payload = build_client_hello_with_extra_extension_before_sni(b"multi.example.org");
    let frame = TlsEncryptedFrame::new(Bytes::from(payload), TlsRecordType::Handshake);
    assert_eq!(frame.routing_key(), Some(b"multi.example.org".as_ref()));
}

#[test]
fn empty_handshake_returns_none() {
    let frame = TlsEncryptedFrame::new(Bytes::new(), TlsRecordType::Handshake);
    assert_eq!(frame.routing_key(), None);
}
