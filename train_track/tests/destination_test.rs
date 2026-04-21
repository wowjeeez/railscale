use bytes::Bytes;
use train_track::StreamDestination;

struct CollectDestination {
    chunks: Vec<Bytes>,
    empty: tokio::io::Empty,
}

impl CollectDestination {
    fn new() -> Self {
        Self { chunks: vec![], empty: tokio::io::empty() }
    }
}

#[async_trait::async_trait]
impl StreamDestination for CollectDestination {
    type Error = std::io::Error;
    type ResponseReader = tokio::io::Empty;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.chunks.push(bytes);
        Ok(())
    }

    fn response_reader(&mut self) -> &mut tokio::io::Empty {
        &mut self.empty
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
