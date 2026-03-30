use bytes::Bytes;
use train_track::{StreamDestination, RailscaleError};

struct CollectDestination {
    chunks: Vec<Bytes>,
}

impl CollectDestination {
    fn new() -> Self {
        Self { chunks: vec![] }
    }
}

#[async_trait::async_trait]
impl StreamDestination for CollectDestination {
    type Error = std::io::Error;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.chunks.push(bytes);
        Ok(())
    }

    async fn relay_response<W: tokio::io::AsyncWrite + Send + Unpin>(&mut self, _client: &mut W) -> Result<u64, Self::Error> {
        Ok(0)
    }
}

#[tokio::test]
async fn destination_write_collects_bytes() {
    let mut dest = CollectDestination::new();

    dest.write(Bytes::from_static(b"hello")).await.unwrap();
    dest.write(Bytes::from_static(b"world")).await.unwrap();

    assert_eq!(dest.chunks.len(), 2);
    assert_eq!(&dest.chunks[0][..], b"hello");
    assert_eq!(&dest.chunks[1][..], b"world");
}
