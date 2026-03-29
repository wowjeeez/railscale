use bytes::{Buf, Bytes, BytesMut};
use memchr::memmem::Finder;
use rayon::prelude::*;
use tokio_util::codec::Decoder;
use crate::HttpFrame;

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
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.done || src.is_empty() {
            return Ok(None);
        }

        match find_crlf(src) {
            Some(0) => {
                src.advance(2);
                self.done = true;
                Ok(None)
            }
            Some(pos) => {
                let is_request_line = self.first_line;
                self.first_line = false;

                let line = src.split_to(pos);
                src.advance(2);

                if is_request_line {
                    return Ok(Some(HttpFrame::header(line.freeze(), true)));
                }

                let replaced = self.matchers.par_iter().find_map_first(|(matcher, value)| {
                    let idx = matcher.find(&line);
                    let sep = memchr::memchr(b':', &line);
                    if idx.is_some() && sep.is_some() {
                        let (name, _) = line.split_at(sep.unwrap());
                        Some(Bytes::from([name, b": ", value].concat()))
                    } else {
                        None
                    }
                });

                match replaced {
                    Some(bytes) => Ok(Some(HttpFrame::header(bytes, false))),
                    None => Ok(Some(HttpFrame::header(line.freeze(), false))),
                }
            }
            None => Ok(None),
        }
    }
}
