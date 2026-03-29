use train_track::{StreamSource, RailscaleError};

struct MockSource {
    data: Vec<u8>,
}

impl StreamSource for MockSource {
    type Stream = tokio::io::DuplexStream;
    type Error = std::io::Error;

    async fn accept(&self) -> Result<Self::Stream, Self::Error> {
        let (client, mut server) = tokio::io::duplex(1024);
        let data = self.data.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            server.write_all(&data).await.unwrap();
            server.shutdown().await.unwrap();
        });
        Ok(client)
    }
}

#[tokio::test]
async fn mock_source_accept() {
    let source = MockSource { data: b"hello".to_vec() };
    let mut stream = source.accept().await.unwrap();
    let mut buf = vec![0u8; 5];
    use tokio::io::AsyncReadExt;
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"hello");
}
