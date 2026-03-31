use crate::atom::frame::Frame;
use crate::atom::switch_rail::SwitchRail;
use crate::core::pipeline::FramePipeline;

pub trait Turnout: Send + Sync {
    type Input: Frame;
    type Output: Frame;
    fn process(&self, input: Self::Input) -> Option<Self::Output>;
}

pub struct SimpleTurnout<R, H> {
    rail: R,
    handler: H,
}

impl<R, H> SimpleTurnout<R, H> {
    pub fn new(rail: R, handler: H) -> Self {
        Self { rail, handler }
    }
}

impl<R, H> Turnout for SimpleTurnout<R, H>
where
    R: SwitchRail,
    H: Fn(R::Output) -> Option<R::Output> + Send + Sync,
{
    type Input = R::Input;
    type Output = R::Output;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let switched = self.rail.switch(input);
        (self.handler)(switched)
    }
}

pub struct FramePipelineAdapter<P>(P);

impl<P> FramePipelineAdapter<P> {
    pub fn new(pipeline: P) -> Self {
        Self(pipeline)
    }
}

impl<P: FramePipeline> Turnout for FramePipelineAdapter<P> {
    type Input = P::Frame;
    type Output = P::Frame;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        Some(self.0.process(input))
    }
}
