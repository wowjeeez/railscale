use std::marker::PhantomData;
use bytes::Bytes;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tokio::io::AsyncWrite;
use crate::{Frame, RailscaleError, StreamDestination};

pub trait DomainMatcher<T: StreamDestination>: Send + Sync {
    fn matches(&self, domain: &[u8]) -> bool;
    fn get_destination(&self) -> T;
}


pub struct RouterDestination<T: Frame, R: StreamDestination<Frame=T>, M: DomainMatcher<R>> {
    target: Option<R>,
    _t: PhantomData<T>,
    matchers: Vec<M>,

}


#[async_trait::async_trait]
impl<T: Frame + Sync, D: StreamDestination<Frame=T>, M: DomainMatcher<D>> StreamDestination for RouterDestination<T, D, M> {
    type Frame = T;
    type Error = RailscaleError;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error> {
        let route = self.matchers.par_iter().find_map_first(|x| {
            if x.matches(&routing_frame.as_bytes()) {
                Some(x.get_destination())
            } else {
                None
            }
        });
        self.target = route;
        Ok(())
    }

    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error> {
        let Some(ref mut target) = self.target else { Err(Self::Error::RoutingFailed("no route".into()))? };
        target.write(frame).await.map_err(Into::into)
    }

    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        let Some(ref mut target) = self.target else { Err(Self::Error::RoutingFailed("no route".into()))? };
        target.write_raw(bytes).await.map_err(Into::into)
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        let Some(ref mut target) = self.target else { Err(Self::Error::RoutingFailed("no route".into()))? };
        target.relay_response::<W>(client).await.map_err(Into::into)
    }
}
