use std::sync::Arc;
use std::time::Duration;
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use train_track::{Pipeline, Service, BufferLimits, RailscaleError, ErrorKind};
use trezorcarriage::{TlsParser, TlsPassthroughPipeline, Passthrough};
use carriage::tcp::native::{TcpSource, TcpRouter};

pub struct ForwardTls {
    source: TcpSource,
    upstream: String,
    buffer_limits: BufferLimits,
    drain_timeout: Duration,
}

impl ForwardTls {
    pub async fn new(bind: &str, upstream: &str) -> Result<Self, RailscaleError> {
        let source = TcpSource::bind(bind).await?;
        Ok(Self {
            source,
            upstream: upstream.to_string(),
            buffer_limits: BufferLimits::default(),
            drain_timeout: Duration::from_secs(30),
        })
    }

    pub fn builder() -> ForwardTlsBuilder {
        ForwardTlsBuilder {
            bind_addr: None,
            upstream: None,
            buffer_limits: BufferLimits::default(),
            drain_timeout: Duration::from_secs(30),
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.source.local_addr()
    }

    pub async fn run(self, cancel: CancellationToken) -> Result<(), RailscaleError> {
        let pipeline = Pipeline {
            source: self.source,
            parser_factory: || TlsParser::new(),
            pipeline: Arc::new(TlsPassthroughPipeline::<Passthrough>::new()),
            router: Arc::new(TcpRouter::fixed(self.upstream)),
            error_responder: None,
            buffer_limits: self.buffer_limits,
            drain_timeout: self.drain_timeout,
        };
        pipeline.run(cancel).await
    }
}

pub struct ForwardTlsBuilder {
    bind_addr: Option<String>,
    upstream: Option<String>,
    buffer_limits: BufferLimits,
    drain_timeout: Duration,
}

impl ForwardTlsBuilder {
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

    pub async fn build(self) -> Result<ForwardTls, RailscaleError> {
        let bind = self.bind_addr.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("bind address required".into()))
        })?;
        let upstream = self.upstream.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("upstream address required".into()))
        })?;
        let source = TcpSource::bind(&bind).await?;
        Ok(ForwardTls {
            source,
            upstream,
            buffer_limits: self.buffer_limits,
            drain_timeout: self.drain_timeout,
        })
    }
}
