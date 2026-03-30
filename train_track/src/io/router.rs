use crate::io::destination::StreamDestination;
use crate::RailscaleError;

#[async_trait::async_trait]
pub trait DestinationRouter: Send + Sync {
    type Destination: StreamDestination;

    async fn route(&self, routing_key: &[u8]) -> Result<Self::Destination, RailscaleError>;
}
