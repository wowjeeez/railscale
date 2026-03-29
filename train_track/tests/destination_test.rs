use bytes::Bytes;
use train_track::{Frame, StreamDestination, RailscaleError};

struct TestFrame(Bytes);

impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn is_routing_frame(&self) -> bool { false }
}

struct CollectDestination {
    frames: Vec<Vec<u8>>,
    raw: Vec<Vec<u8>>,
    routed: bool,
}

impl CollectDestination {
    fn new() -> Self {
        Self { frames: vec![], raw: vec![], routed: false }
    }
}

impl StreamDestination for CollectDestination {
    type Frame = TestFrame;
    type Error = std::io::Error;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error> {
        self.routed = true;
        Ok(())
    }

    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error> {
        self.frames.push(frame.as_bytes().to_vec());
        Ok(())
    }

    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.raw.push(bytes.to_vec());
        Ok(())
    }
}

#[tokio::test]
async fn destination_provide_then_write() {
    let mut dest = CollectDestination::new();
    let routing = TestFrame(Bytes::from_static(b"GET / HTTP/1.1"));
    dest.provide(&routing).await.unwrap();
    assert!(dest.routed);

    dest.write(TestFrame(Bytes::from_static(b"Host: example.com"))).await.unwrap();
    dest.write_raw(Bytes::from_static(b"body data")).await.unwrap();

    assert_eq!(dest.frames.len(), 1);
    assert_eq!(dest.raw.len(), 1);
    assert_eq!(dest.raw[0], b"body data");
}
