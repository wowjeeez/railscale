use std::sync::Arc;
use std::time::Duration;
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use train_track::{Pipeline, Service, BufferLimits, ErrorToBytes, RailscaleError, ErrorKind, StablingConfig};
use carriage::http_v1::{HttpParser, HttpErrorResponder, HttpPipeline, HttpDeriverHook, ResponseParser};
use carriage::tcp::native::TcpRouter;
use trezorcarriage::TlsSource;

pub struct ForwardHttps {
    source: TlsSource,
    upstream: String,
    buffer_limits: BufferLimits,
    drain_timeout: Duration,
    error_responder: Option<Arc<dyn ErrorToBytes + Send + Sync>>,
}

impl ForwardHttps {
    pub async fn new(
        bind: &str,
        upstream: &str,
        tls_config: Arc<rustls::ServerConfig>,
    ) -> Result<Self, RailscaleError> {
        let source = TlsSource::bind(bind, tls_config).await?;
        Ok(Self {
            source,
            upstream: upstream.to_string(),
            buffer_limits: BufferLimits::default(),
            drain_timeout: Duration::from_secs(30),
            error_responder: Some(Arc::new(HttpErrorResponder)),
        })
    }

    pub fn builder() -> ForwardHttpsBuilder {
        ForwardHttpsBuilder {
            bind_addr: None,
            upstream: None,
            tls_config: None,
            buffer_limits: BufferLimits::default(),
            drain_timeout: Duration::from_secs(30),
            error_responder: Some(Arc::new(HttpErrorResponder)),
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.source.local_addr()
    }

    pub async fn run(self, cancel: CancellationToken) -> Result<(), RailscaleError> {
        let pipeline = Pipeline {
            source: self.source,
            parser_factory: || HttpParser::new(vec![]),
            pipeline: Arc::new(HttpPipeline::keepalive(vec![])),
            router: Arc::new(TcpRouter::fixed(self.upstream)),
            error_responder: self.error_responder,
            buffer_limits: self.buffer_limits,
            drain_timeout: self.drain_timeout,
            hook_factory: || HttpDeriverHook::new(),
            response_parser_factory: Some(|| ResponseParser::new()),
            response_pipeline: Some(Arc::new(HttpPipeline::keepalive(vec![]))),
            response_hook_factory: Some(|| HttpDeriverHook::new()),
            stabling_config: Some(StablingConfig::default()),
            turnout_name: "proxy".to_string(),
            capture_dir: None,
        };
        pipeline.run(cancel).await
    }
}

pub struct ForwardHttpsBuilder {
    bind_addr: Option<String>,
    upstream: Option<String>,
    tls_config: Option<Arc<rustls::ServerConfig>>,
    buffer_limits: BufferLimits,
    drain_timeout: Duration,
    error_responder: Option<Arc<dyn ErrorToBytes + Send + Sync>>,
}

impl ForwardHttpsBuilder {
    pub fn bind(mut self, addr: &str) -> Self {
        self.bind_addr = Some(addr.to_string());
        self
    }

    pub fn upstream(mut self, addr: &str) -> Self {
        self.upstream = Some(addr.to_string());
        self
    }

    pub fn tls_config(mut self, config: Arc<rustls::ServerConfig>) -> Self {
        self.tls_config = Some(config);
        self
    }

    pub fn buffer_limits(mut self, limits: BufferLimits) -> Self {
        self.buffer_limits = limits;
        self
    }

    pub fn drain_timeout(mut self, timeout: Duration) -> Self {
        self.drain_timeout = timeout;
        self
    }

    pub fn error_responder(mut self, responder: Arc<dyn ErrorToBytes + Send + Sync>) -> Self {
        self.error_responder = Some(responder);
        self
    }

    pub async fn build(self) -> Result<ForwardHttps, RailscaleError> {
        let bind = self.bind_addr.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("bind address required".into()))
        })?;
        let upstream = self.upstream.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("upstream address required".into()))
        })?;
        let tls_config = self.tls_config.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("TLS server config required".into()))
        })?;
        let source = TlsSource::bind(&bind, tls_config).await?;
        Ok(ForwardHttps {
            source,
            upstream,
            buffer_limits: self.buffer_limits,
            drain_timeout: self.drain_timeout,
            error_responder: self.error_responder,
        })
    }
}
