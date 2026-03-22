use bytes::BytesMut;
use tokio_util::codec::Decoder;

use rail_proto::carriage::passengers::http::HttpPassenger;
use rail_proto::carriage::ticket_pipeline::{BufferedField, PassengerDecoder, TicketField};

fn always_buffer(_: &[u8]) -> bool { true }
fn never_buffer(_: &[u8]) -> bool { false }
fn buffer_user_agent(line: &[u8]) -> bool {
    line.windows(10).any(|w| w.eq_ignore_ascii_case(b"user-agent"))
}

#[test]
fn request_line_always_buffered_as_attribute() {
    let mut decoder = HttpPassenger::with_predicate(never_buffer);
    let mut buf = BytesMut::from("GET /index HTTP/1.1\r\n");
    let result = decoder.decode(&mut buf).unwrap();
    match result {
        Some(TicketField::Buffered(BufferedField::Attribute(attr))) => {
            assert_eq!(attr, "GET /index HTTP/1.1");
        }
        other => panic!("expected Buffered(Attribute), got {:?}", field_debug(&other)),
    }
}

#[test]
fn header_line_parsed_as_key_value() {
    let mut decoder = HttpPassenger::with_predicate(always_buffer);
    // First call consumes request line
    let mut buf = BytesMut::from("GET / HTTP/1.1\r\nHost: example.com\r\n");
    decoder.decode(&mut buf).unwrap(); // request line

    let result = decoder.decode(&mut buf).unwrap();
    match result {
        Some(TicketField::Buffered(BufferedField::Header(key, value))) => {
            assert_eq!(key, "Host");
            assert_eq!(value, "example.com");
        }
        other => panic!("expected Buffered(Header), got {:?}", field_debug(&other)),
    }
}

#[test]
fn header_passthrough_when_predicate_false() {
    let mut decoder = HttpPassenger::with_predicate(never_buffer);
    let mut buf = BytesMut::from("GET / HTTP/1.1\r\nHost: example.com\r\n");
    decoder.decode(&mut buf).unwrap(); // request line (always buffered)

    let result = decoder.decode(&mut buf).unwrap();
    assert!(matches!(result, Some(TicketField::Passthrough(_))));
}

#[test]
fn detects_boundary_crlf() {
    let mut decoder = HttpPassenger::with_predicate(always_buffer);
    let mut buf = BytesMut::from("GET / HTTP/1.1\r\n\r\n");
    decoder.decode(&mut buf).unwrap(); // request line
    let result = decoder.decode(&mut buf).unwrap();
    assert!(matches!(result, Some(TicketField::Boundary)));
}

#[test]
fn detects_boundary_lf_only() {
    let mut decoder = HttpPassenger::with_predicate(always_buffer);
    let mut buf = BytesMut::from("GET / HTTP/1.1\n\n");
    decoder.decode(&mut buf).unwrap(); // request line
    let result = decoder.decode(&mut buf).unwrap();
    assert!(matches!(result, Some(TicketField::Boundary)));
}

#[test]
fn body_after_boundary_returns_none() {
    let mut decoder = HttpPassenger::with_predicate(always_buffer);
    let mut buf = BytesMut::from("GET / HTTP/1.1\r\n\r\n");
    decoder.decode(&mut buf).unwrap(); // request line
    decoder.decode(&mut buf).unwrap(); // boundary

    buf.extend_from_slice(b"body content here");
    let result = decoder.decode(&mut buf).unwrap();
    assert!(result.is_none());
    assert_eq!(&buf[..], b"body content here");
}

#[test]
fn returns_none_on_incomplete_line() {
    let mut decoder = HttpPassenger::with_predicate(always_buffer);
    let mut buf = BytesMut::from("GET /index");
    let result = decoder.decode(&mut buf).unwrap();
    assert!(result.is_none());
    assert_eq!(buf.len(), 10);
}

#[test]
fn selective_buffering_by_header_name() {
    let mut decoder = HttpPassenger::with_predicate(buffer_user_agent);
    let mut buf = BytesMut::from(
        "GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: curl/7.x\r\nAccept: */*\r\n\r\n",
    );

    // Request line — always buffered as Attribute
    let r0 = decoder.decode(&mut buf).unwrap();
    assert!(matches!(r0, Some(TicketField::Buffered(BufferedField::Attribute(_)))));

    // Host — predicate doesn't match, passthrough
    let r1 = decoder.decode(&mut buf).unwrap();
    assert!(matches!(r1, Some(TicketField::Passthrough(_))));

    // User-Agent — predicate matches, parsed as Header
    let r2 = decoder.decode(&mut buf).unwrap();
    match r2 {
        Some(TicketField::Buffered(BufferedField::Header(k, v))) => {
            assert_eq!(k, "User-Agent");
            assert_eq!(v, "curl/7.x");
        }
        other => panic!("expected Header, got {:?}", field_debug(&other)),
    }

    // Accept — passthrough
    let r3 = decoder.decode(&mut buf).unwrap();
    assert!(matches!(r3, Some(TicketField::Passthrough(_))));

    // Boundary
    let r4 = decoder.decode(&mut buf).unwrap();
    assert!(matches!(r4, Some(TicketField::Boundary)));
}

#[test]
fn full_http_request_flow() {
    let mut decoder = HttpPassenger::with_predicate(buffer_user_agent);
    let request = "GET /path HTTP/1.1\r\n\
                   Host: example.com\r\n\
                   User-Agent: test/1.0\r\n\
                   Content-Length: 5\r\n\
                   \r\n\
                   hello";
    let mut buf = BytesMut::from(request);

    // request line — always buffered
    match decoder.decode(&mut buf).unwrap() {
        Some(TicketField::Buffered(BufferedField::Attribute(attr))) => {
            assert_eq!(attr, "GET /path HTTP/1.1");
        }
        other => panic!("expected request line Attribute, got {:?}", field_debug(&other)),
    }
    // Host — passthrough
    assert!(matches!(decoder.decode(&mut buf).unwrap(), Some(TicketField::Passthrough(_))));
    // User-Agent — buffered as Header
    match decoder.decode(&mut buf).unwrap() {
        Some(TicketField::Buffered(BufferedField::Header(k, v))) => {
            assert_eq!(k, "User-Agent");
            assert_eq!(v, "test/1.0");
        }
        other => panic!("expected User-Agent Header, got {:?}", field_debug(&other)),
    }
    // Content-Length — passthrough
    assert!(matches!(decoder.decode(&mut buf).unwrap(), Some(TicketField::Passthrough(_))));
    // boundary
    assert!(matches!(decoder.decode(&mut buf).unwrap(), Some(TicketField::Boundary)));
    // body — decoder returns None, bytes remain in buffer
    assert!(decoder.decode(&mut buf).unwrap().is_none());
    assert!(!buf.is_empty());
}

#[test]
fn empty_body_after_boundary() {
    let mut decoder = HttpPassenger::with_predicate(always_buffer);
    let mut buf = BytesMut::from("GET / HTTP/1.1\r\nHost: x\r\n\r\n");

    decoder.decode(&mut buf).unwrap(); // request line
    decoder.decode(&mut buf).unwrap(); // header
    decoder.decode(&mut buf).unwrap(); // boundary

    let result = decoder.decode(&mut buf).unwrap();
    assert!(result.is_none());
}

#[test]
fn incremental_feed() {
    let mut decoder = HttpPassenger::with_predicate(always_buffer);
    let mut buf = BytesMut::new();

    // feed partial request line
    buf.extend_from_slice(b"GET /ind");
    assert!(decoder.decode(&mut buf).unwrap().is_none());

    // complete request line
    buf.extend_from_slice(b"ex HTTP/1.1\r\n");
    assert!(matches!(
        decoder.decode(&mut buf).unwrap(),
        Some(TicketField::Buffered(BufferedField::Attribute(_)))
    ));

    // feed partial header
    buf.extend_from_slice(b"Host: exa");
    assert!(decoder.decode(&mut buf).unwrap().is_none());

    // complete header
    buf.extend_from_slice(b"mple.com\r\n");
    match decoder.decode(&mut buf).unwrap() {
        Some(TicketField::Buffered(BufferedField::Header(k, v))) => {
            assert_eq!(k, "Host");
            assert_eq!(v, "example.com");
        }
        other => panic!("expected Header, got {:?}", field_debug(&other)),
    }
}

#[test]
fn body_bytes_preserved_for_raw_stream() {
    let mut decoder = HttpPassenger::with_predicate(never_buffer);
    let mut buf = BytesMut::from("GET / HTTP/1.1\r\n\r\n");
    decoder.decode(&mut buf).unwrap(); // request line
    decoder.decode(&mut buf).unwrap(); // boundary

    buf.extend_from_slice(b"chunk1chunk2");
    assert!(decoder.decode(&mut buf).unwrap().is_none());
    assert_eq!(&buf[..], b"chunk1chunk2");
}

fn field_debug(field: &Option<TicketField>) -> &'static str {
    match field {
        None => "None",
        Some(TicketField::Buffered(BufferedField::KeyValue(_, _))) => "Buffered(KeyValue)",
        Some(TicketField::Buffered(BufferedField::Header(_, _))) => "Buffered(Header)",
        Some(TicketField::Buffered(BufferedField::Attribute(_))) => "Buffered(Attribute)",
        Some(TicketField::Buffered(BufferedField::Bytes(_))) => "Buffered(Bytes)",
        Some(TicketField::Passthrough(_)) => "Passthrough",
        Some(TicketField::Boundary) => "Boundary",
    }
}
