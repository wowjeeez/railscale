use bytes::Bytes;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use train_track::StreamDestination;
use carriages::TcpDestination;

#[tokio::test]
async fn writes_to_upstream() {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = upstream.local_addr().unwrap();

    let join = tokio::spawn(async move {
        let (mut conn, _) = upstream.accept().await.unwrap();
        let mut buf = Vec::new();
        conn.read_to_end(&mut buf).await.unwrap();
        buf
    });

    let stream = TcpStream::connect(addr).await.unwrap();
    let mut dest = TcpDestination::new(stream);
    dest.write(Bytes::from_static(b"GET / HTTP/1.1\r\n")).await.unwrap();
    dest.write(Bytes::from_static(b"Host: test\r\n")).await.unwrap();
    dest.write(Bytes::from_static(b"body")).await.unwrap();
    drop(dest);

    let received = join.await.unwrap();
    assert!(received.starts_with(b"GET / HTTP/1.1"));
    assert!(received.ends_with(b"body"));
}
