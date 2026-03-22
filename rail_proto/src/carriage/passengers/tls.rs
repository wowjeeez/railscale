use bytes::BytesMut;
use tokio_util::codec::Decoder;

use crate::carriage::ticket_pipeline::{BufferedField, PassengerDecoder, TicketField};

const TLS_RECORD_HEADER_LEN: usize = 5;
const HANDSHAKE_HEADER_LEN: usize = 4;

// TLS content types
const CONTENT_TYPE_HANDSHAKE: u8 = 22;

// Handshake message types
const HANDSHAKE_CLIENT_HELLO: u8 = 1;

// SNI extension type
const EXT_SERVER_NAME: u16 = 0x0000;

pub struct TlsPassenger {
    past_metadata: bool,
    buffer_predicate: fn(&[u8]) -> bool,
}

impl PassengerDecoder for TlsPassenger {
    fn with_predicate(buffer_predicate: fn(&[u8]) -> bool) -> Self {
        Self {
            past_metadata: false,
            buffer_predicate,
        }
    }
}

impl Decoder for TlsPassenger {
    type Item = TicketField;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.past_metadata {
            return Ok(None);
        }

        if src.len() < TLS_RECORD_HEADER_LEN {
            return Ok(None);
        }

        let content_type = src[0];
        let record_version = u16::from_be_bytes([src[1], src[2]]);
        let record_len = u16::from_be_bytes([src[3], src[4]]) as usize;
        let total_len = TLS_RECORD_HEADER_LEN + record_len;

        if src.len() < total_len {
            return Ok(None);
        }

        if content_type != CONTENT_TYPE_HANDSHAKE {
            self.past_metadata = true;
            return Ok(None);
        }

        let record = src.split_to(total_len);

        if !(self.buffer_predicate)(&record) {
            return Ok(Some(TicketField::Passthrough(record.freeze())));
        }

        let payload = &record[TLS_RECORD_HEADER_LEN..];
        let fields = parse_handshake(payload, record_version);

        // Emit first field as structured, stash rest via Boundary protocol
        // We flatten to a single TicketField per decode call, so emit as combined Attribute
        Ok(Some(TicketField::Buffered(BufferedField::Attribute(
            fields
                .into_iter()
                .map(|f| match f {
                    BufferedField::KeyValue(k, v) => format!("{}={}", k, v),
                    BufferedField::Header(k, v) => format!("{}={}", k, v),
                    BufferedField::Attribute(a) => a,
                    BufferedField::Bytes(_) => String::from("<bytes>"),
                })
                .collect::<Vec<_>>()
                .join("; "),
        ))))
    }
}

fn parse_handshake(payload: &[u8], record_version: u16) -> Vec<BufferedField> {
    let mut fields = Vec::new();

    fields.push(BufferedField::KeyValue(
        "record_version",
        tls_version_str(record_version),
    ));

    if payload.len() < HANDSHAKE_HEADER_LEN {
        return fields;
    }

    let handshake_type = payload[0];
    if handshake_type != HANDSHAKE_CLIENT_HELLO {
        fields.push(BufferedField::KeyValue("handshake_type", "other"));
        return fields;
    }

    fields.push(BufferedField::KeyValue("handshake_type", "client_hello"));

    let handshake_len =
        ((payload[1] as usize) << 16) | ((payload[2] as usize) << 8) | (payload[3] as usize);

    let hello = &payload[HANDSHAKE_HEADER_LEN..];
    if hello.len() < handshake_len || hello.len() < 2 {
        return fields;
    }

    let client_version = u16::from_be_bytes([hello[0], hello[1]]);
    fields.push(BufferedField::KeyValue(
        "client_version",
        tls_version_str(client_version),
    ));

    // Skip: version(2) + random(32) = 34
    let mut cursor = 34;

    // Session ID
    if cursor >= hello.len() {
        return fields;
    }
    let session_id_len = hello[cursor] as usize;
    cursor += 1 + session_id_len;

    // Cipher suites
    if cursor + 2 > hello.len() {
        return fields;
    }
    let cipher_suites_len = u16::from_be_bytes([hello[cursor], hello[cursor + 1]]) as usize;
    let cipher_count = cipher_suites_len / 2;
    fields.push(BufferedField::Header(
        "cipher_suite_count".into(),
        cipher_count.to_string(),
    ));
    cursor += 2 + cipher_suites_len;

    // Compression methods
    if cursor >= hello.len() {
        return fields;
    }
    let compression_len = hello[cursor] as usize;
    cursor += 1 + compression_len;

    // Extensions
    if cursor + 2 > hello.len() {
        return fields;
    }
    let extensions_len = u16::from_be_bytes([hello[cursor], hello[cursor + 1]]) as usize;
    cursor += 2;

    let extensions_end = cursor + extensions_len;
    if extensions_end > hello.len() {
        return fields;
    }

    while cursor + 4 <= extensions_end {
        let ext_type = u16::from_be_bytes([hello[cursor], hello[cursor + 1]]);
        let ext_len = u16::from_be_bytes([hello[cursor + 2], hello[cursor + 3]]) as usize;
        cursor += 4;

        if ext_type == EXT_SERVER_NAME && ext_len > 0 {
            if let Some(sni) = parse_sni(&hello[cursor..cursor + ext_len]) {
                fields.push(BufferedField::Header("sni".into(), sni));
            }
        }

        cursor += ext_len;
    }

    fields
}

fn parse_sni(data: &[u8]) -> Option<String> {
    // SNI extension: list_length(2) + entry_type(1) + name_length(2) + name
    if data.len() < 5 {
        return None;
    }
    let entry_type = data[2];
    if entry_type != 0 {
        // 0 = host_name
        return None;
    }
    let name_len = u16::from_be_bytes([data[3], data[4]]) as usize;
    if data.len() < 5 + name_len {
        return None;
    }
    String::from_utf8(data[5..5 + name_len].to_vec()).ok()
}

fn tls_version_str(version: u16) -> &'static str {
    match version {
        0x0300 => "ssl3.0",
        0x0301 => "tls1.0",
        0x0302 => "tls1.1",
        0x0303 => "tls1.2",
        0x0304 => "tls1.3",
        _ => "unknown",
    }
}
