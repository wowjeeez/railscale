use std::marker::PhantomData;
use crate::atom::frame::Frame;
use crate::io::departure::{Departure, StreamDeparture};
use crate::io::router::DestinationRouter;
use crate::RailscaleError;

#[async_trait::async_trait]
pub trait Shunt: Send + Sync {
    type Input: Frame;
    type Departure: Departure;
    async fn connect(&self, routing_key: &[u8]) -> Result<Self::Departure, RailscaleError>;
}

pub struct RouterShunt<F, R> {
    router: R,
    _frame: PhantomData<F>,
}

impl<F, R> RouterShunt<F, R> {
    pub fn new(router: R) -> Self {
        Self {
            router,
            _frame: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<F, R> Shunt for RouterShunt<F, R>
where
    F: Frame,
    R: DestinationRouter,
{
    type Input = F;
    type Departure = StreamDeparture<R::Destination>;

    async fn connect(&self, routing_key: &[u8]) -> Result<Self::Departure, RailscaleError> {
        let dest = self.router.route(routing_key).await?;
        Ok(StreamDeparture::new(dest))
    }
}
