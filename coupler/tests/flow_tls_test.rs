use train_track::BufferLimits;
use coupler::ForwardTls;

#[tokio::test]
async fn forward_tls_creates_with_new() {
    let flow = ForwardTls::new("127.0.0.1:0", "127.0.0.1:1").await.unwrap();
    assert_ne!(flow.local_addr().port(), 0);
}

#[tokio::test]
async fn forward_tls_builder_works() {
    let flow = ForwardTls::builder()
        .bind("127.0.0.1:0")
        .upstream("127.0.0.1:1")
        .build()
        .await
        .unwrap();
    assert_ne!(flow.local_addr().port(), 0);
}

#[tokio::test]
async fn forward_tls_builder_fails_without_bind() {
    let result = ForwardTls::builder()
        .upstream("127.0.0.1:443")
        .build()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn forward_tls_builder_fails_without_upstream() {
    let result = ForwardTls::builder()
        .bind("127.0.0.1:0")
        .build()
        .await;
    assert!(result.is_err());
}
