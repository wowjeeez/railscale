use bytes::Bytes;
use memchr::memmem::Finder;
use train_track::{Turnout, FramePipelineAdapter};
use crate::http_v1::HttpFrame;
use crate::http::pipeline::HttpPipeline;

pub struct HttpTurnout(FramePipelineAdapter<HttpPipeline>);

impl HttpTurnout {
    pub fn new(replacements: Vec<(Finder<'static>, Bytes)>) -> Self {
        Self(FramePipelineAdapter::new(HttpPipeline::new(replacements)))
    }

    pub fn passthrough() -> Self {
        Self(FramePipelineAdapter::new(HttpPipeline::new(vec![])))
    }
}

impl Turnout for HttpTurnout {
    type Input = HttpFrame;
    type Output = HttpFrame;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        self.0.process(input)
    }
}
