use std::marker::PhantomData;
use train_track::FramePipeline;
use crate::frame::TlsEncryptedFrame;

pub struct Passthrough;
pub struct Terminate;
pub struct Decrypt;

pub struct TlsPassthroughPipeline<Mode> {
    _mode: PhantomData<Mode>,
}

impl<Mode> TlsPassthroughPipeline<Mode> {
    pub fn new() -> Self {
        Self { _mode: PhantomData }
    }
}

impl FramePipeline for TlsPassthroughPipeline<Passthrough> {
    type Frame = TlsEncryptedFrame;

    fn process(&self, frame: Self::Frame) -> Self::Frame {
        frame
    }
}
