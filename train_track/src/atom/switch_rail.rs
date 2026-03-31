use std::marker::PhantomData;
use crate::atom::frame::Frame;

pub trait SwitchRail: Send + Sync {
    type Input: Frame;
    type Output: Frame;
    fn switch(&self, input: Self::Input) -> Self::Output;
}

pub struct IdentityRail<F>(PhantomData<F>);

impl<F> Default for IdentityRail<F> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<F> IdentityRail<F> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<F: Frame> SwitchRail for IdentityRail<F> {
    type Input = F;
    type Output = F;
    fn switch(&self, input: F) -> F { input }
}
