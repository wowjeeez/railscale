use std::sync::Arc;
use train_track::{Frame, Shunt, RouterShunt, StreamDeparture, RailscaleError};
use trezorcarriage::{TlsClientDestination, TlsClientRouter};

pub struct OverTls<F>(RouterShunt<F, TlsClientRouter>);

impl<F> OverTls<F> {
    pub fn fixed(addr: impl Into<String>, config: Arc<rustls::ClientConfig>) -> Self {
        Self(RouterShunt::new(TlsClientRouter::fixed(addr.into(), config)))
    }

    pub fn from_routing_key(config: Arc<rustls::ClientConfig>) -> Self {
        Self(RouterShunt::new(TlsClientRouter::from_routing_key(config)))
    }
}

#[async_trait::async_trait]
impl<F: Frame> Shunt for OverTls<F> {
    type Input = F;
    type Departure = StreamDeparture<TlsClientDestination>;

    async fn connect(&self, routing_key: &[u8]) -> Result<Self::Departure, RailscaleError> {
        self.0.connect(routing_key).await
    }
}
