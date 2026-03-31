use bytes::Bytes;
use train_track::{Departure, StreamDeparture, StreamDestination, Transload, ChannelTransload, RailscaleError};

struct MockDestination {
    written: Vec<Bytes>,
}

impl MockDestination {
    fn new() -> Self {
        Self { written: Vec::new() }
    }
}

#[async_trait::async_trait]
impl StreamDestination for MockDestination {
    type Error = RailscaleError;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.written.push(bytes);
        Ok(())
    }

    async fn relay_response<W: tokio::io::AsyncWrite + Send + Unpin>(
        &mut self,
        _client: &mut W,
    ) -> Result<u64, Self::Error> {
        Ok(0)
    }
}

#[tokio::test]
async fn stream_departure_delegates_write() {
    let mock = MockDestination::new();
    let mut departure = StreamDeparture::new(mock);
    departure.depart(Bytes::from_static(b"hello")).await.unwrap();
    departure.depart(Bytes::from_static(b"world")).await.unwrap();
}

#[tokio::test]
async fn stream_departure_delegates_relay() {
    let mock = MockDestination::new();
    let mut departure = StreamDeparture::new(mock);
    let mut buf = Vec::new();
    let bytes = departure.relay_response(&mut buf).await.unwrap();
    assert_eq!(bytes, 0);
}

#[tokio::test]
async fn channel_transload_sends_bytes() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut transload = ChannelTransload::new(tx);
    transload.depart(Bytes::from_static(b"data")).await.unwrap();
    let received = rx.recv().await.unwrap();
    assert_eq!(&received[..], b"data");
}

#[tokio::test]
async fn channel_transload_errors_on_closed_channel() {
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);
    drop(rx);
    let mut transload = ChannelTransload::new(tx);
    let result = transload.depart(Bytes::from_static(b"data")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn channel_transload_relay_returns_zero() {
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let mut transload = ChannelTransload::new(tx);
    let mut buf = Vec::new();
    let bytes = transload.relay_response(&mut buf).await.unwrap();
    assert_eq!(bytes, 0);
}

#[test]
fn departure_types_are_send() {
    fn assert_send<T: Send>() {}
    assert_send::<StreamDeparture<MockDestination>>();
    assert_send::<ChannelTransload>();
}
