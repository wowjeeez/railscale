use bytes::BytesMut;
use tokio_util::codec::Decoder;

use rail_proto::carriage::passengers::tls::TlsPassenger;
use rail_proto::carriage::ticket_pipeline::{BufferedField, PassengerDecoder, TicketField};

fn always_buffer(_: &[u8]) -> bool { true }
fn never_buffer(_: &[u8]) -> bool { false }

/// Build a fake TLS record: type(1) + version(2) + length(2) + payload
fn make_tls_record(content_type: u8, payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u16;
    let mut record = vec![content_type, 0x03, 0x03]; // type + TLS 1.2
    record.extend_from_slice(&len.to_be_bytes());
    record.extend_from_slice(payload);
    record
}

#[test]
fn incomplete_header_returns_none() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);
    let mut buf = BytesMut::from(&[0x16, 0x03, 0x03][..]);
    assert!(decoder.decode(&mut buf).unwrap().is_none());
}

#[test]
fn incomplete_record_returns_none() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);
    let record = make_tls_record(22, b"client hello data");
    let mut buf = BytesMut::from(&record[..8]);
    assert!(decoder.decode(&mut buf).unwrap().is_none());
}

#[test]
fn handshake_record_buffered() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);
    let record = make_tls_record(22, b"client hello data");
    let mut buf = BytesMut::from(record.as_slice());

    let result = decoder.decode(&mut buf).unwrap();
    assert!(matches!(result, Some(TicketField::Buffered(_))));
    assert!(buf.is_empty());
}

#[test]
fn handshake_record_passthrough_when_predicate_false() {
    let mut decoder = TlsPassenger::with_predicate(never_buffer);
    let record = make_tls_record(22, b"client hello data");
    let mut buf = BytesMut::from(record.as_slice());

    let result = decoder.decode(&mut buf).unwrap();
    assert!(matches!(result, Some(TicketField::Passthrough(_))));
}

#[test]
fn application_data_ends_decoding() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);

    let record = make_tls_record(23, b"encrypted payload");
    let expected_len = record.len();
    let mut buf = BytesMut::from(record.as_slice());

    let result = decoder.decode(&mut buf).unwrap();
    assert!(result.is_none());
    // app data record is put back for raw stream
    assert_eq!(buf.len(), expected_len);
}

#[test]
fn handshake_then_app_data_stops_decoding() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);

    let handshake = make_tls_record(22, b"hello");
    let app_data = make_tls_record(23, b"encrypted");
    let app_data_len = app_data.len();

    let mut buf = BytesMut::new();
    buf.extend_from_slice(&handshake);
    buf.extend_from_slice(&app_data);

    // handshake — buffered
    assert!(matches!(decoder.decode(&mut buf).unwrap(), Some(TicketField::Buffered(_))));
    // app data — returns None, record put back in buffer
    assert!(decoder.decode(&mut buf).unwrap().is_none());
    assert_eq!(buf.len(), app_data_len);
}

#[test]
fn multiple_handshake_records_then_stop() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);

    let hs1 = make_tls_record(22, b"client hello");
    let hs2 = make_tls_record(22, b"server hello");
    let app = make_tls_record(23, b"data");
    let app_len = app.len();

    let mut buf = BytesMut::new();
    buf.extend_from_slice(&hs1);
    buf.extend_from_slice(&hs2);
    buf.extend_from_slice(&app);

    assert!(matches!(decoder.decode(&mut buf).unwrap(), Some(TicketField::Buffered(_))));
    assert!(matches!(decoder.decode(&mut buf).unwrap(), Some(TicketField::Buffered(_))));
    // app data stops decoding, record preserved
    assert!(decoder.decode(&mut buf).unwrap().is_none());
    assert_eq!(buf.len(), app_len);
}

#[test]
fn after_metadata_phase_returns_none() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);

    let app1 = make_tls_record(23, b"first");
    let mut buf = BytesMut::from(app1.as_slice());

    // first app data ends metadata phase, record put back
    assert!(decoder.decode(&mut buf).unwrap().is_none());
    // subsequent calls also return None
    assert!(decoder.decode(&mut buf).unwrap().is_none());
}

#[test]
fn zero_length_payload_record() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);
    let record = make_tls_record(22, b"");
    let mut buf = BytesMut::from(record.as_slice());

    let result = decoder.decode(&mut buf).unwrap();
    assert!(matches!(result, Some(TicketField::Buffered(_))));
}

#[test]
fn incremental_record_feed() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);
    let record = make_tls_record(22, b"incremental data");
    let mut buf = BytesMut::new();

    for (i, &byte) in record.iter().enumerate() {
        buf.extend_from_slice(&[byte]);
        let result = decoder.decode(&mut buf).unwrap();
        if i < record.len() - 1 {
            assert!(result.is_none(), "should be None at byte {i}");
        } else {
            assert!(matches!(result, Some(TicketField::Buffered(_))));
        }
    }
}

#[test]
fn never_produces_boundary() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);
    let records = [
        make_tls_record(22, b"handshake"),
        make_tls_record(22, b"another handshake"),
    ];

    let mut buf = BytesMut::new();
    for r in &records {
        buf.extend_from_slice(r);
    }

    while let Some(field) = decoder.decode(&mut buf).unwrap() {
        assert!(!matches!(field, TicketField::Boundary));
    }
}

#[test]
fn app_data_bytes_preserved_for_raw_stream() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);

    let hs = make_tls_record(22, b"handshake");
    let app = make_tls_record(23, b"important encrypted data");
    let app_clone = app.clone();

    let mut buf = BytesMut::new();
    buf.extend_from_slice(&hs);
    buf.extend_from_slice(&app);

    // consume handshake
    decoder.decode(&mut buf).unwrap();
    // app data stops decoding
    decoder.decode(&mut buf).unwrap();
    // verify the exact app data record is preserved
    assert_eq!(&buf[..], &app_clone[..]);
}

/// Build a minimal but valid TLS ClientHello with SNI extension
fn make_client_hello(sni_hostname: &str) -> Vec<u8> {
    let hostname_bytes = sni_hostname.as_bytes();

    // SNI extension payload: list_len(2) + type(1) + name_len(2) + name
    let sni_entry_len = 1 + 2 + hostname_bytes.len(); // type + name_len + name
    let sni_list_len = sni_entry_len;
    let mut sni_ext = Vec::new();
    sni_ext.extend_from_slice(&(sni_list_len as u16).to_be_bytes()); // server_name_list length
    sni_ext.push(0x00); // host_name type
    sni_ext.extend_from_slice(&(hostname_bytes.len() as u16).to_be_bytes());
    sni_ext.extend_from_slice(hostname_bytes);

    // Extensions block: ext_type(2) + ext_len(2) + ext_data
    let mut extensions = Vec::new();
    extensions.extend_from_slice(&0x0000u16.to_be_bytes()); // SNI extension type
    extensions.extend_from_slice(&(sni_ext.len() as u16).to_be_bytes());
    extensions.extend_from_slice(&sni_ext);

    // ClientHello body
    let mut hello_body = Vec::new();
    hello_body.extend_from_slice(&[0x03, 0x03]); // client version TLS 1.2
    hello_body.extend_from_slice(&[0u8; 32]); // random
    hello_body.push(0x00); // session_id length = 0
    hello_body.extend_from_slice(&[0x00, 0x02]); // cipher_suites length = 2
    hello_body.extend_from_slice(&[0x00, 0x2F]); // TLS_RSA_WITH_AES_128_CBC_SHA
    hello_body.push(0x01); // compression_methods length = 1
    hello_body.push(0x00); // null compression
    hello_body.extend_from_slice(&(extensions.len() as u16).to_be_bytes());
    hello_body.extend_from_slice(&extensions);

    // Handshake header: type(1) + length(3)
    let hello_len = hello_body.len();
    let mut handshake = Vec::new();
    handshake.push(0x01); // ClientHello
    handshake.push(((hello_len >> 16) & 0xFF) as u8);
    handshake.push(((hello_len >> 8) & 0xFF) as u8);
    handshake.push((hello_len & 0xFF) as u8);
    handshake.extend_from_slice(&hello_body);

    // Wrap in TLS record
    make_tls_record(22, &handshake)
}

#[test]
fn client_hello_extracts_sni() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);
    let record = make_client_hello("api.railscale.dev");
    let mut buf = BytesMut::from(record.as_slice());

    let result = decoder.decode(&mut buf).unwrap();
    match result {
        Some(TicketField::Buffered(BufferedField::Attribute(attr))) => {
            assert!(attr.contains("sni=api.railscale.dev"), "expected SNI in: {attr}");
            assert!(attr.contains("client_version=tls1.2"), "expected TLS version in: {attr}");
            assert!(attr.contains("handshake_type=client_hello"), "expected handshake type in: {attr}");
            assert!(attr.contains("cipher_suite_count=1"), "expected cipher count in: {attr}");
        }
        other => panic!("expected Buffered(Attribute) with SNI, got: {other:?}"),
    }
}

#[test]
fn client_hello_without_sni() {
    let mut decoder = TlsPassenger::with_predicate(always_buffer);

    // Build a ClientHello with no extensions
    let mut hello_body = Vec::new();
    hello_body.extend_from_slice(&[0x03, 0x03]); // TLS 1.2
    hello_body.extend_from_slice(&[0u8; 32]); // random
    hello_body.push(0x00); // session_id = 0
    hello_body.extend_from_slice(&[0x00, 0x02, 0x00, 0x2F]); // 1 cipher suite
    hello_body.extend_from_slice(&[0x01, 0x00]); // 1 compression method (null)
    hello_body.extend_from_slice(&[0x00, 0x00]); // extensions length = 0

    let hello_len = hello_body.len();
    let mut handshake = vec![0x01]; // ClientHello type
    handshake.push(((hello_len >> 16) & 0xFF) as u8);
    handshake.push(((hello_len >> 8) & 0xFF) as u8);
    handshake.push((hello_len & 0xFF) as u8);
    handshake.extend_from_slice(&hello_body);

    let record = make_tls_record(22, &handshake);
    let mut buf = BytesMut::from(record.as_slice());

    let result = decoder.decode(&mut buf).unwrap();
    match result {
        Some(TicketField::Buffered(BufferedField::Attribute(attr))) => {
            assert!(!attr.contains("sni="), "should not have SNI: {attr}");
            assert!(attr.contains("client_version=tls1.2"));
            assert!(attr.contains("handshake_type=client_hello"));
        }
        other => panic!("expected Buffered(Attribute), got: {other:?}"),
    }
}
