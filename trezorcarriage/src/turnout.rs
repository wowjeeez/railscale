use train_track::{Frame, Turnout, SwitchRail, FramePipelineAdapter};
use crate::frame::TlsEncryptedFrame;
use crate::pipeline::{TlsPassthroughPipeline, Passthrough};
use carriage::http_v1::HttpFrame;

pub struct TlsPassthroughTurnout(FramePipelineAdapter<TlsPassthroughPipeline<Passthrough>>);

impl TlsPassthroughTurnout {
    pub fn new() -> Self {
        Self(FramePipelineAdapter::new(TlsPassthroughPipeline::<Passthrough>::new()))
    }
}

impl Turnout for TlsPassthroughTurnout {
    type Input = TlsEncryptedFrame;
    type Output = TlsEncryptedFrame;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        self.0.process(input)
    }
}

pub struct TlsTerminationRail;

impl SwitchRail for TlsTerminationRail {
    type Input = TlsEncryptedFrame;
    type Output = HttpFrame;

    fn switch(&self, input: TlsEncryptedFrame) -> HttpFrame {
        HttpFrame::header(input.into_bytes())
    }
}
