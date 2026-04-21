use bytes::Bytes;
use memchr::memmem::Finder;
use rayon::prelude::*;
use train_track::{Frame, FramePipeline, PhasedFrame};
use crate::http_v1::{HttpFrame, HttpPhase};

const HOP_BY_HOP: &[&[u8]] = &[
    b"connection",
    b"keep-alive",
    b"proxy-connection",
    b"proxy-authenticate",
    b"proxy-authorization",
    b"te",
    b"upgrade",
];

fn is_hop_by_hop(header_name: &[u8]) -> bool {
    HOP_BY_HOP.iter().any(|h| header_name.eq_ignore_ascii_case(h))
}

pub struct HttpPipeline {
    matchers: Vec<(Finder<'static>, Bytes)>,
    inject_close: bool,
}

impl HttpPipeline {
    pub fn new(matchers: Vec<(Finder<'static>, Bytes)>) -> Self {
        Self { matchers, inject_close: true }
    }

    pub fn keepalive(matchers: Vec<(Finder<'static>, Bytes)>) -> Self {
        Self { matchers, inject_close: false }
    }
}

impl FramePipeline for HttpPipeline {
    type Frame = HttpFrame;

    fn process(&self, frame: Self::Frame) -> Self::Frame {
        if frame.phase() == HttpPhase::EndOfHeaders {
            if self.inject_close {
                return HttpFrame::header(Bytes::from_static(b"Connection: close\r\n\r\n"));
            }
            return frame;
        }

        if frame.phase() != HttpPhase::Header {
            return frame;
        }

        let line = frame.as_bytes();
        let sep = match memchr::memchr(b':', line) {
            Some(s) => s,
            None => return frame,
        };
        let name = &line[..sep];

        if is_hop_by_hop(name) {
            return HttpFrame::header(Bytes::new());
        }

        if self.matchers.is_empty() {
            return frame;
        }

        let replaced = self.matchers.par_iter().find_map_first(|(matcher, value)| {
            matcher.find(name).map(|_| {
                Bytes::from([name, b": ", &value[..], b"\r\n"].concat())
            })
        });

        match replaced {
            Some(bytes) => HttpFrame::header(bytes),
            None => frame,
        }
    }
}
