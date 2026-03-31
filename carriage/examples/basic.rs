use bytes::Bytes;
use memchr::memmem::Finder;
use std::sync::Arc;
use carriages::http_v1::{HttpParser, HttpPipeline};
use carriages::init_metrics;
use carriages::tcp::native::{TcpRouter, TcpSource};
use train_track::{CancellationToken, Pipeline, Service};

#[tokio::main]
async fn main() {
    let listen = std::env::args().nth(1).unwrap_or("127.0.0.1:8080".into());
    let upstream = std::env::args().nth(2).unwrap_or("127.0.0.1:9090".into());

    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let _meter_provider = init_metrics();

    let source = TcpSource::bind(&listen).await.unwrap();

    let pipeline = Pipeline {
        source,
        parser_factory: || HttpParser::new(vec![]),
        pipeline: Arc::new(HttpPipeline::new(vec![
            (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
        ])),
        router: Arc::new(TcpRouter::fixed(&upstream)),
        error_responder: Some(Arc::new(carriages::http_v1::HttpErrorResponder)),
        buffer_limits: Default::default(),
        drain_timeout: std::time::Duration::from_secs(30),
        #[cfg(feature = "metrics-full")]
        recorder: None,
    };

    pipeline.run(CancellationToken::new()).await.unwrap();
}
