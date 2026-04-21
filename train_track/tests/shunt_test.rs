use bytes::Bytes;
use train_track::{Frame, Shunt, RouterShunt, Departure, StreamDestination, DestinationRouter, RailscaleError};

struct TestFrame(Bytes);

impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { Some(&self.0) }
}

struct MockDestination {
    empty: tokio::io::Empty,
}

impl MockDestination {
    fn new() -> Self { Self { empty: tokio::io::empty() } }
}

#[async_trait::async_trait]
impl StreamDestination for MockDestination {
    type Error = RailscaleError;
    type ResponseReader = tokio::io::Empty;

    async fn write(&mut self, _bytes: Bytes) -> Result<(), Self::Error> {
        Ok(())
    }

    fn response_reader(&mut self) -> &mut tokio::io::Empty {
        &mut self.empty
    }
}

struct MockRouter;

#[async_trait::async_trait]
impl DestinationRouter for MockRouter {
    type Destination = MockDestination;

    async fn route(&self, _routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        Ok(MockDestination::new())
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
    let _reader = dep.response_reader();
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
