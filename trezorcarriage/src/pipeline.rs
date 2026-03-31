use std::marker::PhantomData;

pub struct Passthrough;
pub struct Terminate;
pub struct Decrypt;

pub struct TlsPassthroughPipeline<Mode> {
    _mode: PhantomData<Mode>,
}
