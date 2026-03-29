use bytes::Bytes;
use tokio::io::AsyncReadExt;
use train_track::{Frame, StreamDestination};
use locomotive::{HttpFrame, TcpDestination};

#[tokio::test]
async fn provides_and_writes_to_upstream() {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = upstream.local_addr().unwrap();

    let join = tokio::spawn(async move {
        let (mut conn, _) = upstream.accept().await.unwrap();
        let mut buf = Vec::new();
        conn.read_to_end(&mut buf).await.unwrap();
        buf
    });

    let mut dest = TcpDestination::new();
    let routing = HttpFrame::header(
        Bytes::from(format!("GET / HTTP/1.1\r\nUpstream: {addr}")),
        true,
    );
    dest.provide_with_addr(&addr).await.unwrap();
    dest.write(routing).await.unwrap();
    dest.write(HttpFrame::header(Bytes::from_static(b"Host: test\r\n"), false)).await.unwrap();
    dest.write_raw(Bytes::from_static(b"body")).await.unwrap();
    drop(dest);

    let received = join.await.unwrap();
    assert!(received.starts_with(b"GET / HTTP/1.1"));
    assert!(received.ends_with(b"body"));
}
