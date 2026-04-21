use bytes::{Buf, Bytes, BytesMut};
use tokio_util::codec::Decoder;
use std::io;
use train_track::DeriverSession;
use crate::http_v1::HttpFrame;
use crate::http_v1::HttpPhase;
use crate::http_v1::derive::{Matcher, HttpDerivationInput, BodyFramingMode};

fn extract_status_line_version(line: &[u8]) -> &[u8] {
    match memchr::memchr(b' ', line) {
        Some(pos) => &line[..pos],
        None => line,
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    let mut start = 0;
    loop {
        match memchr::memchr(b'\n', &buf[start..]) {
            Some(pos) => {
                let abs = start + pos;
                if abs > 0 && buf[abs - 1] == b'\r' {
                    return Some(abs - 1);
                }
                start = abs + 1;
            }
            None => return None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyState {
    Headers,
    FixedLength { remaining: usize },
    Chunked(ChunkedState),
    UntilEof,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkedState {
    ExpectSize,
    Data { remaining: usize },
    ExpectDataCrlf,
    TrailerOrEnd,
}

pub struct ResponseCodec {
    body_state: BodyState,
    first_line: bool,
    session: DeriverSession<Matcher>,
    body_framing: Option<BodyFramingMode>,
}

impl ResponseCodec {
    pub fn new() -> Self {
        Self {
            body_state: BodyState::Headers,
            first_line: true,
            session: DeriverSession::new(HttpDerivationInput::all_matchers()),
            body_framing: None,
        }
    }

    pub fn is_response_complete(&self) -> bool {
        self.body_state == BodyState::Done
    }

    pub fn body_framing_mode(&self) -> Option<BodyFramingMode> {
        self.body_framing
    }

    pub fn session(&self) -> &DeriverSession<Matcher> {
        &self.session
    }

    fn decode_headers(&mut self, src: &mut BytesMut) -> Result<Option<HttpFrame>, io::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        match find_crlf(src) {
            Some(0) => {
                src.advance(2);
                let derived = HttpDerivationInput::resolve_all(&self.session);
                self.body_framing = Some(derived.body_framing);
                self.body_state = match derived.body_framing {
                    BodyFramingMode::Fixed(0) | BodyFramingMode::None => BodyState::Done,
                    BodyFramingMode::Fixed(n) => BodyState::FixedLength { remaining: n },
                    BodyFramingMode::Chunked => BodyState::Chunked(ChunkedState::ExpectSize),
                    BodyFramingMode::UntilClose | BodyFramingMode::Invalid => BodyState::UntilEof,
                };
                Ok(Some(HttpFrame::end_of_headers()))
            }
            Some(pos) => {
                let is_status = self.first_line;
                self.first_line = false;

                if !is_status && (src[0] == b' ' || src[0] == b'\t') {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "obs-fold in response header (RFC 9112 §5.2)",
                    ));
                }

                let line = src.split_to(pos + 2).freeze();

                if is_status {
                    self.session.feed(&HttpPhase::StatusLine, &line);
                    let version = extract_status_line_version(&line);
                    let synthetic = [b"GET / ".as_ref(), version, b"\r\n"].concat();
                    self.session.feed(&HttpPhase::RequestLine, &synthetic);
                    return Ok(Some(HttpFrame::status_line(line)));
                }

                self.session.feed(&HttpPhase::Header, &line);
                Ok(Some(HttpFrame::header(line)))
            }
            None => Ok(None),
        }
    }

    fn decode_fixed_length(&mut self, src: &mut BytesMut, remaining: usize) -> Option<Bytes> {
        if src.is_empty() {
            return None;
        }
        let to_take = remaining.min(src.len());
        let chunk = src.split_to(to_take).freeze();
        let new_remaining = remaining - to_take;
        if new_remaining == 0 {
            self.body_state = BodyState::Done;
        } else {
            self.body_state = BodyState::FixedLength { remaining: new_remaining };
        }
        Some(chunk)
    }

    fn decode_chunked(&mut self, src: &mut BytesMut, state: ChunkedState) -> Result<Option<Bytes>, io::Error> {
        match state {
            ChunkedState::ExpectSize => {
                let crlf = match find_crlf(src) {
                    Some(pos) => pos,
                    None => return Ok(None),
                };
                let size_line = &src[..crlf];
                let size_str = std::str::from_utf8(size_line)
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid chunk size"))?;
                let size_str = size_str.split(';').next().unwrap_or(size_str).trim();
                let size = usize::from_str_radix(size_str, 16)
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid chunk size hex"))?;

                let chunk_header = src.split_to(crlf + 2).freeze();

                if size == 0 {
                    self.body_state = BodyState::Chunked(ChunkedState::TrailerOrEnd);
                    return Ok(Some(chunk_header));
                }

                self.body_state = BodyState::Chunked(ChunkedState::Data { remaining: size });
                Ok(Some(chunk_header))
            }
            ChunkedState::Data { remaining } => {
                if src.is_empty() {
                    return Ok(None);
                }
                let to_take = remaining.min(src.len());
                let chunk = src.split_to(to_take).freeze();
                let new_remaining = remaining - to_take;
                if new_remaining == 0 {
                    self.body_state = BodyState::Chunked(ChunkedState::ExpectDataCrlf);
                } else {
                    self.body_state = BodyState::Chunked(ChunkedState::Data { remaining: new_remaining });
                }
                Ok(Some(chunk))
            }
            ChunkedState::ExpectDataCrlf => {
                if src.len() < 2 {
                    return Ok(None);
                }
                let crlf = src.split_to(2).freeze();
                self.body_state = BodyState::Chunked(ChunkedState::ExpectSize);
                Ok(Some(crlf))
            }
            ChunkedState::TrailerOrEnd => {
                match find_crlf(src) {
                    Some(0) => {
                        let end = src.split_to(2).freeze();
                        self.body_state = BodyState::Done;
                        Ok(Some(end))
                    }
                    Some(pos) => {
                        let trailer = src.split_to(pos + 2).freeze();
                        Ok(Some(trailer))
                    }
                    None => Ok(None),
                }
            }
        }
    }
}

pub enum ResponseCodecItem {
    Frame(HttpFrame),
    Body(Bytes),
}

impl Decoder for ResponseCodec {
    type Item = ResponseCodecItem;
    type Error = io::Error;

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.body_state {
            BodyState::UntilEof => {
                if buf.is_empty() {
                    self.body_state = BodyState::Done;
                    return Ok(None);
                }
                let chunk = buf.split().freeze();
                self.body_state = BodyState::Done;
                Ok(Some(ResponseCodecItem::Body(chunk)))
            }
            BodyState::Done => Ok(None),
            _ => self.decode(buf),
        }
    }

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, io::Error> {
        match self.body_state {
            BodyState::Headers => {
                match self.decode_headers(src)? {
                    Some(frame) => Ok(Some(ResponseCodecItem::Frame(frame))),
                    None => Ok(None),
                }
            }
            BodyState::FixedLength { remaining } => {
                match self.decode_fixed_length(src, remaining) {
                    Some(bytes) => Ok(Some(ResponseCodecItem::Body(bytes))),
                    None => Ok(None),
                }
            }
            BodyState::Chunked(state) => {
                match self.decode_chunked(src, state)? {
                    Some(bytes) => Ok(Some(ResponseCodecItem::Body(bytes))),
                    None => Ok(None),
                }
            }
            BodyState::UntilEof => {
                if src.is_empty() {
                    return Ok(None);
                }
                let chunk = src.split().freeze();
                Ok(Some(ResponseCodecItem::Body(chunk)))
            }
            BodyState::Done => Ok(None),
        }
    }
}
