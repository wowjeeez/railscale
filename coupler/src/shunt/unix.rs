use train_track::{Frame, Shunt, RouterShunt, StreamDeparture, RailscaleError};
use carriage::tcp::unix_sockets::{TcpOverSockDestination, TcpOverSockRouter};

pub struct OverUnix<F>(RouterShunt<F, TcpOverSockRouter>);

impl<F> OverUnix<F> {
    pub fn new(path: impl Into<String>) -> Self {
        Self(RouterShunt::new(TcpOverSockRouter::new(path)))
    }
}

#[async_trait::async_trait]
impl<F: Frame> Shunt for OverUnix<F> {
    type Input = F;
    type Departure = StreamDeparture<TcpOverSockDestination>;

    async fn connect(&self, routing_key: &[u8]) -> Result<Self::Departure, RailscaleError> {
        self.0.connect(routing_key).await
    }
}
