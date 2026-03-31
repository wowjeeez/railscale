use std::sync::Arc;
use std::time::Duration;
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use train_track::{Pipeline, Service, BufferLimits, ErrorToBytes, RailscaleError, ErrorKind};
use carriage::http_v1::{HttpParser, HttpErrorResponder, HttpPipeline};
use carriage::tcp::native::{TcpSource, TcpRouter};

pub struct ForwardHttp {
    source: TcpSource,
    upstream: String,
    buffer_limits: BufferLimits,
    drain_timeout: Duration,
    error_responder: Option<Arc<dyn ErrorToBytes + Send + Sync>>,
}

impl ForwardHttp {
    pub async fn new(bind: &str, upstream: &str) -> Result<Self, RailscaleError> {
        let source = TcpSource::bind(bind).await?;
        Ok(Self {
            source,
            upstream: upstream.to_string(),
            buffer_limits: BufferLimits::default(),
            drain_timeout: Duration::from_secs(30),
            error_responder: Some(Arc::new(HttpErrorResponder)),
        })
    }

    pub fn builder() -> ForwardHttpBuilder {
        ForwardHttpBuilder {
            bind_addr: None,
            upstream: None,
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
            pipeline: Arc::new(HttpPipeline::new(vec![])),
            router: Arc::new(TcpRouter::fixed(self.upstream)),
            error_responder: self.error_responder,
            buffer_limits: self.buffer_limits,
            drain_timeout: self.drain_timeout,
        };
        pipeline.run(cancel).await
    }
}

pub struct ForwardHttpBuilder {
    bind_addr: Option<String>,
    upstream: Option<String>,
    buffer_limits: BufferLimits,
    drain_timeout: Duration,
    error_responder: Option<Arc<dyn ErrorToBytes + Send + Sync>>,
}

impl ForwardHttpBuilder {
    pub fn bind(mut self, addr: &str) -> Self {
        self.bind_addr = Some(addr.to_string());
        self
    }

    pub fn upstream(mut self, addr: &str) -> Self {
        self.upstream = Some(addr.to_string());
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

    pub async fn build(self) -> Result<ForwardHttp, RailscaleError> {
        let bind = self.bind_addr.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("bind address required".into()))
        })?;
        let upstream = self.upstream.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("upstream address required".into()))
        })?;
        let source = TcpSource::bind(&bind).await?;
        Ok(ForwardHttp {
            source,
            upstream,
            buffer_limits: self.buffer_limits,
            drain_timeout: self.drain_timeout,
            error_responder: self.error_responder,
        })
    }
}
