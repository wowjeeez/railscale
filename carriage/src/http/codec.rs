use bytes::{Buf, Bytes, BytesMut};
use memchr::memmem::Finder;
use rayon::prelude::*;
use tokio_util::codec::Decoder;
use std::io;
use crate::http_v1::HttpFrame;

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

pub struct HttpStreamingCodec {
    done: bool,
    first_line: bool,
    matchers: Vec<(Finder<'static>, Bytes)>,
}

impl HttpStreamingCodec {
    pub fn new(matchers: Vec<(Finder<'static>, Bytes)>) -> Self {
        Self { done: false, first_line: true, matchers }
    }

    pub fn headers_done(&self) -> bool {
        self.done
    }
}

impl Decoder for HttpStreamingCodec {
    type Item = HttpFrame;
    type Error = io::Error;

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.done {
            return Ok(None);
        }
        self.decode(buf)
    }

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, io::Error> {
        if self.done || src.is_empty() {
            return Ok(None);
        }

        match find_crlf(src) {
            Some(0) => {
                src.advance(2);
                self.done = true;
                Ok(Some(HttpFrame::end_of_headers()))
            }
            Some(pos) => {
                let is_request_line = self.first_line;
                self.first_line = false;

                let line = src.split_to(pos + 2).freeze();

                if is_request_line {
                    return Ok(Some(HttpFrame::request_line(line)));
                }

                let header = &line[..line.len() - 2];
                let replaced = self.matchers.par_iter().find_map_first(|(matcher, value)| {
                    let sep = memchr::memchr(b':', header)?;
                    let name = &header[..sep];
                    matcher.find(name).map(|_| {
                        Bytes::from([name, b": ", value.as_ref(), b"\r\n"].concat())
                    })
                });

                match replaced {
                    Some(bytes) => Ok(Some(HttpFrame::header(bytes))),
                    None => Ok(Some(HttpFrame::header(line))),
                }
            }
            None => Ok(None),
        }
    }
}
