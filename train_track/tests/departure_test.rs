use bytes::Bytes;
use train_track::{Departure, StreamDeparture, StreamDestination, Transload, ChannelTransload, RailscaleError};

struct MockDestination {
    written: Vec<Bytes>,
    empty: tokio::io::Empty,
}

impl MockDestination {
    fn new() -> Self {
        Self { written: Vec::new(), empty: tokio::io::empty() }
    }
}

#[async_trait::async_trait]
impl StreamDestination for MockDestination {
    type Error = RailscaleError;
    type ResponseReader = tokio::io::Empty;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.written.push(bytes);
        Ok(())
    }

    fn response_reader(&mut self) -> &mut tokio::io::Empty {
        &mut self.empty
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
async fn stream_departure_exposes_response_reader() {
    let mock = MockDestination::new();
    let mut departure = StreamDeparture::new(mock);
    let _reader = departure.response_reader();
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
async fn channel_transload_exposes_response_reader() {
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let mut transload = ChannelTransload::new(tx);
    let _reader = transload.response_reader();
}

#[test]
fn departure_types_are_send() {
    fn assert_send<T: Send>() {}
    assert_send::<StreamDeparture<MockDestination>>();
    assert_send::<ChannelTransload>();
}
