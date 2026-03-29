use crate::atom::frame::Frame;

pub trait FramePipeline: Send + Sync {
    type Frame: Frame;
    fn process(&self, frame: Self::Frame) -> Self::Frame;
}
