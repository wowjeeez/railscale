use proptest::prelude::*;

pub fn arb_method() -> impl Strategy<Value = &'static [u8]> {
    prop_oneof![
        Just(&b"GET"[..]),
        Just(&b"POST"[..]),
        Just(&b"PUT"[..]),
        Just(&b"DELETE"[..]),
        Just(&b"HEAD"[..]),
        Just(&b"OPTIONS"[..]),
        Just(&b"PATCH"[..]),
    ]
}

pub fn arb_uri() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0x21u8..=0x7Eu8, 1..64)
        .prop_filter("no spaces in URI", |v| !v.contains(&b' '))
        .prop_map(|mut v| {
            v[0] = b'/';
            v
        })
}

pub fn arb_version() -> impl Strategy<Value = &'static [u8]> {
    prop_oneof![Just(&b"HTTP/1.0"[..]), Just(&b"HTTP/1.1"[..]),]
}

pub fn arb_request_line() -> impl Strategy<Value = Vec<u8>> {
    (arb_method(), arb_uri(), arb_version()).prop_map(|(method, uri, version)| {
        let mut line = Vec::with_capacity(method.len() + 1 + uri.len() + 1 + version.len() + 2);
        line.extend_from_slice(method);
        line.push(b' ');
        line.extend_from_slice(&uri);
        line.push(b' ');
        line.extend_from_slice(version);
        line.extend_from_slice(b"\r\n");
        line
    })
}

fn tchar() -> impl Strategy<Value = u8> {
    prop_oneof![
        b'a'..=b'z',
        b'A'..=b'Z',
        b'0'..=b'9',
        Just(b'!'),
        Just(b'#'),
        Just(b'$'),
        Just(b'%'),
        Just(b'&'),
        Just(b'\''),
        Just(b'*'),
        Just(b'+'),
        Just(b'-'),
        Just(b'.'),
        Just(b'^'),
        Just(b'_'),
        Just(b'`'),
        Just(b'|'),
        Just(b'~'),
    ]
}

pub fn arb_header_name() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(tchar(), 1..=32)
}

pub fn arb_header_value() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0x20u8..=0x7Eu8, 0..=128)
}

pub fn arb_header_line() -> impl Strategy<Value = Vec<u8>> {
    (arb_header_name(), arb_header_value()).prop_map(|(name, value)| {
        let mut line = Vec::with_capacity(name.len() + 2 + value.len() + 2);
        line.extend_from_slice(&name);
        line.extend_from_slice(b": ");
        line.extend_from_slice(&value);
        line.extend_from_slice(b"\r\n");
        line
    })
}

pub fn arb_content_length() -> impl Strategy<Value = Vec<u8>> {
    (0u64..10_000_000).prop_map(|n| format!("Content-Length: {n}\r\n").into_bytes())
}

pub fn arb_transfer_encoding() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        Just(b"Transfer-Encoding: chunked\r\n".to_vec()),
        Just(b"Transfer-Encoding: gzip, chunked\r\n".to_vec()),
        Just(b"Transfer-Encoding: deflate, chunked\r\n".to_vec()),
        Just(b"Transfer-Encoding: gzip\r\n".to_vec()),
        Just(b"Transfer-Encoding: identity\r\n".to_vec()),
    ]
}

pub fn arb_connection() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        Just(b"Connection: keep-alive\r\n".to_vec()),
        Just(b"Connection: close\r\n".to_vec()),
        Just(b"Connection: keep-alive, upgrade\r\n".to_vec()),
        Just(b"Connection: close, keep-alive\r\n".to_vec()),
    ]
}

pub fn arb_http_message() -> impl Strategy<Value = Vec<u8>> {
    (
        arb_request_line(),
        prop::collection::vec(arb_header_line(), 0..10),
    )
        .prop_map(|(request_line, headers)| {
            let mut msg = request_line;
            for header in headers {
                msg.extend_from_slice(&header);
            }
            msg.extend_from_slice(b"\r\n");
            msg
        })
}

pub fn arb_tls_record_type() -> impl Strategy<Value = u8> {
    prop_oneof![Just(20u8), Just(21u8), Just(22u8), Just(23u8)]
}

pub fn arb_tls_record(
    record_type: impl Strategy<Value = u8>,
    payload_len: impl Strategy<Value = usize>,
) -> impl Strategy<Value = Vec<u8>> {
    (record_type, payload_len).prop_flat_map(|(rt, len)| {
        prop::collection::vec(any::<u8>(), len).prop_map(move |payload| {
            let mut record = Vec::with_capacity(5 + payload.len());
            record.push(rt);
            record.extend_from_slice(&[0x03, 0x03]);
            record.push((payload.len() >> 8) as u8);
            record.push((payload.len() & 0xff) as u8);
            record.extend_from_slice(&payload);
            record
        })
    })
}

pub fn arb_hostname() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-z][a-z0-9]{0,10}", 1..=4)
        .prop_map(|labels| labels.join("."))
}

pub fn arb_tls_client_hello(
    hostname: impl Strategy<Value = String>,
) -> impl Strategy<Value = (String, Vec<u8>)> {
    (hostname, 0u8..32, 1usize..16, 0usize..6).prop_map(
        |(host, sid_len, cs_count, extra_ext_count)| {
            let hostname_bytes = host.as_bytes();
            let name_len = hostname_bytes.len();
            let server_name_list_len = 1 + 2 + name_len;
            let sni_data_len = 2 + server_name_list_len;

            let mut extensions = Vec::new();
            for i in 0..extra_ext_count {
                let ext_type = (0xff01u16 + i as u16).to_be_bytes();
                extensions.extend_from_slice(&ext_type);
                extensions.extend_from_slice(&[0x00, 0x02, 0xde, 0xad]);
            }
            extensions.extend_from_slice(&[0x00, 0x00]);
            extensions.push((sni_data_len >> 8) as u8);
            extensions.push((sni_data_len & 0xff) as u8);
            extensions.push((server_name_list_len >> 8) as u8);
            extensions.push((server_name_list_len & 0xff) as u8);
            extensions.push(0x00);
            extensions.push((name_len >> 8) as u8);
            extensions.push((name_len & 0xff) as u8);
            extensions.extend_from_slice(hostname_bytes);

            let cs_bytes_len = cs_count * 2;
            let body_len = 2
                + 32
                + 1
                + sid_len as usize
                + 2
                + cs_bytes_len
                + 1
                + 1
                + 2
                + extensions.len();

            let mut handshake = Vec::new();
            handshake.push(0x01);
            handshake.push(((body_len >> 16) & 0xff) as u8);
            handshake.push(((body_len >> 8) & 0xff) as u8);
            handshake.push((body_len & 0xff) as u8);
            handshake.extend_from_slice(&[0x03, 0x03]);
            handshake.extend_from_slice(&[0u8; 32]);
            handshake.push(sid_len);
            handshake.extend(std::iter::repeat(0u8).take(sid_len as usize));
            handshake.push((cs_bytes_len >> 8) as u8);
            handshake.push((cs_bytes_len & 0xff) as u8);
            for _ in 0..cs_count {
                handshake.extend_from_slice(&[0x00, 0x9c]);
            }
            handshake.push(0x01);
            handshake.push(0x00);
            handshake.push((extensions.len() >> 8) as u8);
            handshake.push((extensions.len() & 0xff) as u8);
            handshake.extend_from_slice(&extensions);

            let mut record = Vec::new();
            record.push(0x16);
            record.extend_from_slice(&[0x03, 0x01]);
            record.push((handshake.len() >> 8) as u8);
            record.push((handshake.len() & 0xff) as u8);
            record.extend_from_slice(&handshake);

            (host, record)
        },
    )
}

pub fn arb_tls_stream() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(arb_tls_record(arb_tls_record_type(), 1usize..256), 1..=10)
}

pub fn arb_invalid_tls_record_type() -> impl Strategy<Value = u8> {
    any::<u8>().prop_filter("must not be valid TLS record type", |&b| {
        !matches!(b, 20 | 21 | 22 | 23)
    })
}
