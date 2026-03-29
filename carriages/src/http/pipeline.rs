use bytes::Bytes;
use memchr::memmem::Finder;
use rayon::prelude::*;
use train_track::{Frame, FramePipeline};
use crate::HttpFrame;

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
            let idx = matcher.find(line);
            let sep = memchr::memchr(b':', line);
            if idx.is_some() && sep.is_some() {
                let (name, _) = line.split_at(sep.unwrap());
                Some(Bytes::from([name, b": ", &value[..]].concat()))
            } else {
                None
            }
        });

        match replaced {
            Some(bytes) => HttpFrame::header(bytes, frame.is_routing_frame()),
            None => frame,
        }
    }
}
