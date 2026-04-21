use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::time::Duration;
use bogie::harness::*;

#[tokio::test]
async fn keepalive_two_requests_one_connection() {
    let upstream = TestUpstream::multi_response(200, "OK", "hello").await;
    let proxy = TestProxy::new_with_keepalive(&upstream.addr.to_string()).await;

    let mut client = TcpStream::connect(proxy.addr).await.unwrap();

    client.write_all(b"GET /first HTTP/1.1\r\nHost: example.com\r\n\r\n").await.unwrap();
    let mut buf = vec![0u8; 4096];
    let n = tokio::time::timeout(
        Duration::from_secs(2),
        client.read(&mut buf),
    ).await.unwrap().unwrap();
    let response1 = String::from_utf8_lossy(&buf[..n]);
    assert!(response1.contains("200"), "first response: {response1}");
    assert!(response1.contains("hello"), "first body: {response1}");

    client.write_all(b"GET /second HTTP/1.1\r\nHost: example.com\r\n\r\n").await.unwrap();
    let n = tokio::time::timeout(
        Duration::from_secs(2),
        client.read(&mut buf),
    ).await.unwrap().unwrap();
    let response2 = String::from_utf8_lossy(&buf[..n]);
    assert!(response2.contains("200"), "second response: {response2}");
    assert!(response2.contains("hello"), "second body: {response2}");
}

#[tokio::test]
async fn connection_close_header_stops_keepalive() {
    let upstream = TestUpstream::multi_response(200, "OK", "done").await;
    let proxy = TestProxy::new_with_keepalive(&upstream.addr.to_string()).await;

    let mut client = TcpStream::connect(proxy.addr).await.unwrap();

    client.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n").await.unwrap();

    let mut buf = Vec::new();
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        client.read_to_end(&mut buf),
    ).await;
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("200"), "response: {response}");

    let write_result = client.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    if write_result.is_ok() {
        let mut buf2 = vec![0u8; 4096];
        let read_result = tokio::time::timeout(
            Duration::from_millis(500),
            client.read(&mut buf2),
        ).await;
        match read_result {
            Ok(Ok(0)) => {},
            Ok(Ok(_)) => panic!("should not get a second response after Connection: close"),
            Ok(Err(_)) => {},
            Err(_) => {},
        }
    }
}
