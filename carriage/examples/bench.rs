use bytes::Bytes;
use memchr::memmem::Finder;
use carriage::http_v1::{HttpParser, HttpPipeline};
use carriage::init_metrics;
use carriage::tcp::native::{TcpRouter, TcpSource};
use std::sync::Arc;
use train_track::{CancellationToken, NoHook, Pipeline, Service};
#[cfg(feature = "metrics-full")]
use train_track::recorder::start_recorder;

#[tokio::main]
async fn main() {
    let listen = std::env::args().nth(1).unwrap_or("127.0.0.1:8080".into());
    let upstream = std::env::args().nth(2).unwrap_or("127.0.0.1:9090".into());
    #[cfg(feature = "metrics-full")]
    let output = std::env::args().nth(3).unwrap_or("bench.jsonl".into());

    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"))
        )
        .init();

    let _meter_provider = init_metrics();

    #[cfg(feature = "metrics-full")]
    eprintln!("recording to {output}");
    eprintln!("listening on {listen}, forwarding to {upstream}");

    #[cfg(feature = "metrics-full")]
    let recorder = Arc::new(start_recorder(&output));
    let source = TcpSource::bind(&listen).await.unwrap();

    let pipeline = Pipeline {
        source,
        parser_factory: || HttpParser::new(vec![]),
        pipeline: Arc::new(HttpPipeline::new(vec![
            (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale/1.0")),
        ])),
        router: Arc::new(TcpRouter::fixed(&upstream)),
        error_responder: Some(Arc::new(carriage::http_v1::HttpErrorResponder)),
        buffer_limits: Default::default(),
        drain_timeout: std::time::Duration::from_secs(30),
        hook_factory: || NoHook,
        response_parser_factory: None::<fn() -> HttpParser>,
        response_pipeline: None,
        response_hook_factory: None,
        stabling_config: None,
            turnout_name: "proxy".to_string(),
            capture_dir: None,
        #[cfg(feature = "metrics-full")]
        recorder: Some(recorder),
    };

    pipeline.run(CancellationToken::new()).await.unwrap();
}
