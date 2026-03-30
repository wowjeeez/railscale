use bytes::Bytes;
use memchr::memmem::Finder;
use carriages::{HttpParser, HttpPipeline, TcpRouter, TcpSource};
use std::sync::Arc;
use train_track::{Pipeline, Service};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter("warn")
        .init();

    let source = TcpSource::bind("127.0.0.1:8080").await.unwrap();

    let pipeline = Pipeline {
        source,
        parser_factory: || HttpParser::new(vec![]),
        pipeline: Arc::new(HttpPipeline::new(vec![
            (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
        ])),
        router: Arc::new(TcpRouter::fixed("127.0.0.1:9090")),
        #[cfg(feature = "metrics-full")]
        sampler: None,
    };

    pipeline.run().await.unwrap();
}
