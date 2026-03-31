use bytes::Bytes;
use train_track::{Frame, Shunt, RouterShunt, Departure, StreamDestination, DestinationRouter, RailscaleError};

struct TestFrame(Bytes);

impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { Some(&self.0) }
}

struct MockDestination;

#[async_trait::async_trait]
impl StreamDestination for MockDestination {
    type Error = RailscaleError;

    async fn write(&mut self, _bytes: Bytes) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn relay_response<W: tokio::io::AsyncWrite + Send + Unpin>(
        &mut self,
        _client: &mut W,
    ) -> Result<u64, Self::Error> {
        Ok(42)
    }
}

struct MockRouter;

#[async_trait::async_trait]
impl DestinationRouter for MockRouter {
    type Destination = MockDestination;

    async fn route(&self, _routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        Ok(MockDestination)
    }
}

struct FailRouter;

#[async_trait::async_trait]
impl DestinationRouter for FailRouter {
    type Destination = MockDestination;

    async fn route(&self, _routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        Err(RailscaleError::from(train_track::ErrorKind::RoutingFailed("no route".into())))
    }
}

#[tokio::test]
async fn router_shunt_connects_via_router() {
    let shunt = RouterShunt::<TestFrame, _>::new(MockRouter);
    let mut dep = shunt.connect(b"test-key").await.unwrap();
    let mut buf = Vec::new();
    let relayed = dep.relay_response(&mut buf).await.unwrap();
    assert_eq!(relayed, 42);
}

#[tokio::test]
async fn router_shunt_propagates_routing_errors() {
    let shunt = RouterShunt::<TestFrame, _>::new(FailRouter);
    let result = shunt.connect(b"test-key").await;
    assert!(result.is_err());
}

#[test]
fn router_shunt_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RouterShunt<TestFrame, MockRouter>>();
}
