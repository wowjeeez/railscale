use bytes::BytesMut;
use tokio_util::codec::Decoder;

use crate::carriage::ticket_pipeline::{BufferedField, PassengerDecoder, TicketField};

pub struct HttpPassenger {
    past_metadata: bool,
    seen_request_line: bool,
    buffer_predicate: fn(&[u8]) -> bool,
}

impl PassengerDecoder for HttpPassenger {
    fn with_predicate(buffer_predicate: fn(&[u8]) -> bool) -> Self {
        Self {
            past_metadata: false,
            seen_request_line: false,
            buffer_predicate,
        }
    }
}

impl Decoder for HttpPassenger {
    type Item = TicketField;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.past_metadata {
            return Ok(None);
        }

        let newline_pos = src.iter().position(|&b| b == b'\n');
        match newline_pos {
            Some(pos) => {
                let line_bytes = src.split_to(pos + 1);
                let line = strip_crlf(&line_bytes);

                if line.is_empty() {
                    self.past_metadata = true;
                    return Ok(Some(TicketField::Boundary));
                }

                if !self.seen_request_line {
                    self.seen_request_line = true;
                    return Ok(Some(parse_request_line(line)));
                }

                if !(self.buffer_predicate)(&line_bytes) {
                    return Ok(Some(TicketField::Passthrough(line_bytes.freeze())));
                }

                Ok(Some(parse_header_line(line)))
            }
            None => Ok(None),
        }
    }
}

fn strip_crlf(raw: &[u8]) -> &[u8] {
    let mut end = raw.len();
    if end > 0 && raw[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && raw[end - 1] == b'\r' {
        end -= 1;
    }
    &raw[..end]
}

fn parse_request_line(line: &[u8]) -> TicketField {
    // "GET /path HTTP/1.1"
    let line_str = String::from_utf8_lossy(line);
    let mut parts = line_str.splitn(3, ' ');

    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("");

    TicketField::Buffered(BufferedField::Attribute(
        format!("{} {} {}", method, path, version),
    ))
}

fn parse_header_line(line: &[u8]) -> TicketField {
    let line_str = String::from_utf8_lossy(line);

    if let Some(colon_pos) = line_str.find(':') {
        let key = line_str[..colon_pos].trim().to_string();
        let value = line_str[colon_pos + 1..].trim().to_string();
        TicketField::Buffered(BufferedField::Header(key, value))
    } else {
        // Malformed header line — buffer as raw attribute
        TicketField::Buffered(BufferedField::Attribute(line_str.into_owned()))
    }
}
