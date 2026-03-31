use bytes::Bytes;
use memchr::memmem::Finder;
use rayon::prelude::*;
use train_track::{Frame, FramePipeline};
use crate::http_v1::HttpFrame;

pub struct HttpPipeline {
    matchers: Vec<(Finder<'static>, Bytes)>,
}

impl HttpPipeline {
    pub fn new(matchers: Vec<(Finder<'static>, Bytes)>) -> Self {
        Self { matchers }
    }
}

impl FramePipeline for HttpPipeline {
    type Frame = HttpFrame;

    fn process(&self, frame: Self::Frame) -> Self::Frame {
        if self.matchers.is_empty() {
            return frame;
        }

        let line = frame.as_bytes();
        let replaced = self.matchers.par_iter().find_map_first(|(matcher, value)| {
            let sep = memchr::memchr(b':', line)?;
            let name = &line[..sep];
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
