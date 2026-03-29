use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use locomotive::TcpSource;
use train_track::StreamSource;

#[tokio::test]
async fn tcp_source_accepts_connection() {
    let source = TcpSource::bind("127.0.0.1:0").await.unwrap();
    let addr = source.local_addr();

    let join = tokio::spawn(async move {
        source.accept().await.unwrap()
    });

    let mut client = TcpStream::connect(addr).await.unwrap();
    use tokio::io::AsyncWriteExt;
    client.write_all(b"hello").await.unwrap();
    client.shutdown().await.unwrap();

    let mut server_stream = join.await.unwrap();
    let mut buf = vec![0u8; 5];
    server_stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"hello");
}
