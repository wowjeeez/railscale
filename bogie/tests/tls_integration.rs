use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;

use bogie::harness::*;

#[tokio::test]
async fn tls_simple_get_200() {
    install_crypto();
    let upstream = TestUpstream::fixed_response(200, "OK", "hello").await;
    let proxy = TestTlsProxy::new(&upstream.addr.to_string()).await;
    let response = send_tls_raw(
        proxy.addr,
        proxy.client_config.clone(),
        "localhost",
        b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
    assert_body_contains(&response, "hello");
}

#[tokio::test]
async fn tls_post_with_content_length() {
    install_crypto();
    let upstream = TestUpstream::fixed_response(200, "OK", "accepted").await;
    let proxy = TestTlsProxy::new(&upstream.addr.to_string()).await;
    let response = send_tls_raw(
        proxy.addr,
        proxy.client_config.clone(),
        "localhost",
        b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nhello",
    )
    .await;
    assert_status(&response, 200);
    assert_body_contains(&response, "accepted");
}

#[tokio::test]
async fn tls_large_body_streams() {
    install_crypto();
    let upstream = TestUpstream::fixed_response(200, "OK", "uploaded").await;
    let proxy = TestTlsProxy::new(&upstream.addr.to_string()).await;
    let body = vec![b'X'; 1024 * 1024];
    let headers = format!(
        "POST /upload HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let mut request = headers.into_bytes();
    request.extend_from_slice(&body);

    let tcp = TcpStream::connect(proxy.addr).await.unwrap();
    let connector = TlsConnector::from(proxy.client_config.clone());
    let server_name = ServerName::try_from("localhost".to_string()).unwrap();
    let mut tls = connector.connect(server_name, tcp).await.unwrap();
    let _ = tls.write_all(&request).await;
    let _ = tls.shutdown().await;
    let mut response = Vec::new();
    let _ =
        tokio::time::timeout(Duration::from_secs(5), tls.read_to_end(&mut response)).await;
    assert_status(&response, 200);
}

#[tokio::test]
async fn tls_concurrent_clients() {
    install_crypto();
    let upstream = TestUpstream::fixed_response(200, "OK", "ok").await;
    let proxy = TestTlsProxy::new(&upstream.addr.to_string()).await;
    let mut handles = Vec::new();
    for _ in 0..10 {
        let addr = proxy.addr;
        let config = proxy.client_config.clone();
        handles.push(tokio::spawn(async move {
            let response = send_tls_raw(
                addr,
                config,
                "localhost",
                b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
            )
            .await;
            assert_status(&response, 200);
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn tls_client_disconnect_mid_handshake() {
    install_crypto();
    let upstream = TestUpstream::fixed_response(200, "OK", "alive").await;
    let upstream_addr = upstream.addr.to_string();
    {
        let proxy = TestTlsProxy::new(&upstream_addr).await;
        let mut tcp = TcpStream::connect(proxy.addr).await.unwrap();
        tcp.write_all(&[0x16, 0x03, 0x01]).await.unwrap();
        drop(tcp);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let proxy2 = TestTlsProxy::new(&upstream_addr).await;
    let response = send_tls_raw(
        proxy2.addr,
        proxy2.client_config.clone(),
        "localhost",
        b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
    assert_body_contains(&response, "alive");
}

#[tokio::test]
async fn tls_client_disconnect_mid_request() {
    install_crypto();
    let upstream = TestUpstream::fixed_response(200, "OK", "alive").await;
    let proxy = TestTlsProxy::new(&upstream.addr.to_string()).await;
    {
        let tcp = TcpStream::connect(proxy.addr).await.unwrap();
        let connector = TlsConnector::from(proxy.client_config.clone());
        let server_name = ServerName::try_from("localhost".to_string()).unwrap();
        let mut tls = connector.connect(server_name, tcp).await.unwrap();
        tls.write_all(b"GET / HTTP/1.1\r\nHost:").await.unwrap();
        drop(tls);
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let response = send_tls_raw(
        proxy.addr,
        proxy.client_config.clone(),
        "localhost",
        b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .await;
    assert_status(&response, 200);
    assert_body_contains(&response, "alive");
}

#[tokio::test]
async fn tls_upstream_down_returns_502() {
    install_crypto();
    let proxy = TestTlsProxy::new("127.0.0.1:1").await;
    let response = send_tls_raw(
        proxy.addr,
        proxy.client_config.clone(),
        "localhost",
        b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .await;
    assert_status(&response, 502);
}

#[tokio::test]
async fn tls_malformed_http_over_tls_returns_400() {
    install_crypto();
    let upstream = TestUpstream::echo().await;
    let proxy = TestTlsProxy::new(&upstream.addr.to_string()).await;
    let response = send_tls_raw(
        proxy.addr,
        proxy.client_config.clone(),
        "localhost",
        b"not http at all",
    )
    .await;
    assert_status(&response, 400);
}

#[tokio::test]
async fn plaintext_to_tls_port_rejected() {
    install_crypto();
    let upstream = TestUpstream::fixed_response(200, "OK", "hello").await;
    let proxy = TestTlsProxy::new(&upstream.addr.to_string()).await;
    let response =
        send_raw(proxy.addr, b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let is_tls_alert = response.first() == Some(&0x15);
    assert!(
        response.is_empty() || is_tls_alert,
        "expected empty or TLS alert from TLS port when sending plaintext, got {} bytes: {:?}",
        response.len(),
        &response
    );
}

#[tokio::test]
async fn tls_sni_mismatch_behavior() {
    install_crypto();
    let upstream = TestUpstream::fixed_response(200, "OK", "hello").await;
    let proxy = TestTlsProxy::new(&upstream.addr.to_string()).await;

    let tcp = TcpStream::connect(proxy.addr).await.unwrap();
    let connector = TlsConnector::from(proxy.client_config.clone());
    let server_name = ServerName::try_from("wrong.example.com".to_string()).unwrap();
    let result = connector.connect(server_name, tcp).await;
    assert!(
        result.is_err(),
        "expected TLS handshake to fail with SNI mismatch, but it succeeded"
    );
}
