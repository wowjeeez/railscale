use train_track::{Frame, Shunt, RouterShunt, StreamDeparture, RailscaleError};
use carriage::tcp::native::{TcpDestination, TcpRouter};

pub struct OverTcp<F>(RouterShunt<F, TcpRouter>);

impl<F> OverTcp<F> {
    pub fn fixed(addr: impl Into<String>) -> Self {
        Self(RouterShunt::new(TcpRouter::fixed(addr)))
    }

    pub fn from_routing_key() -> Self {
        Self(RouterShunt::new(TcpRouter::from_routing_key()))
    }
}

#[async_trait::async_trait]
impl<F: Frame> Shunt for OverTcp<F> {
    type Input = F;
    type Departure = StreamDeparture<TcpDestination>;

    async fn connect(&self, routing_key: &[u8]) -> Result<Self::Departure, RailscaleError> {
        self.0.connect(routing_key).await
    }
}
