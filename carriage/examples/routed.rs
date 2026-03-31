use std::path::PathBuf;
use std::sync::Arc;
use carriage::http_v1::{HttpParser, HttpPipeline};
use carriage::init_metrics;
use carriage::tcp::native::{TcpRouter, TcpSource};
use train_track::{CancellationToken, DestinationRouter, FileDestination, Pipeline, RailscaleError, Service};

struct FileRouter;

#[async_trait::async_trait]
impl DestinationRouter for FileRouter {
    type Destination = FileDestination;

    async fn route(&self, _routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        FileDestination::new(PathBuf::from("output.bin")).map_err(Into::into)
    }
}

#[tokio::main]
async fn main() {
    let listen = std::env::args().nth(1).unwrap_or("127.0.0.1:8080".into());

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
        pipeline: Arc::new(HttpPipeline::new(vec![])),
        router: Arc::new(FileRouter),
        error_responder: None,
        buffer_limits: Default::default(),
        drain_timeout: std::time::Duration::from_secs(30),
        #[cfg(feature = "metrics-full")]
        recorder: None,
    };

    pipeline.run(CancellationToken::new()).await.unwrap();
}
