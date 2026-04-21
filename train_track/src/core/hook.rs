use crate::atom::frame::Frame;
use crate::RailscaleError;

pub trait ConnectionHook<F: Frame>: Send + 'static {
    fn on_frame(&mut self, frame: &F);
    fn validate(&self) -> Result<(), RailscaleError> {
        Ok(())
    }
    fn reset(&mut self) {}
    fn should_close_connection(&self) -> bool { false }
}

pub struct NoHook;

impl<F: Frame> ConnectionHook<F> for NoHook {
    fn on_frame(&mut self, _frame: &F) {}
}
