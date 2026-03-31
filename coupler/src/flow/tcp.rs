use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use train_track::{BufferLimits, ErrorKind, RailscaleError, StreamSource};
use carriage::tcp::native::TcpSource;

pub struct ForwardTcp {
    source: TcpSource,
    upstream: String,
    drain_timeout: Duration,
}

impl ForwardTcp {
    pub async fn new(bind: &str, upstream: &str) -> Result<Self, RailscaleError> {
        let source = TcpSource::bind(bind).await?;
        Ok(Self {
            source,
            upstream: upstream.to_string(),
            drain_timeout: Duration::from_secs(30),
        })
    }

    pub fn builder() -> ForwardTcpBuilder {
        ForwardTcpBuilder {
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
        let mut join_set: JoinSet<()> = JoinSet::new();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                result = self.source.accept() => {
                    let (cr, cw) = result?;
                    let upstream_addr = self.upstream.clone();
                    join_set.spawn(async move {
                        if let Ok(upstream) = TcpStream::connect(&upstream_addr).await {
                            let (mut ur, mut uw) = upstream.into_split();
                            let mut cr = cr;
                            let mut cw = cw;
                            let _ = tokio::join!(
                                tokio::io::copy(&mut cr, &mut uw),
                                tokio::io::copy(&mut ur, &mut cw),
                            );
                        }
                    });
                }
            }
        }
        let drain_timeout = self.drain_timeout;
        tokio::select! {
            _ = async { while join_set.join_next().await.is_some() {} } => {}
            _ = tokio::time::sleep(drain_timeout) => {
                join_set.abort_all();
            }
        }
        Ok(())
    }
}

pub struct ForwardTcpBuilder {
    bind_addr: Option<String>,
    upstream: Option<String>,
    buffer_limits: BufferLimits,
    drain_timeout: Duration,
}

impl ForwardTcpBuilder {
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

    pub async fn build(self) -> Result<ForwardTcp, RailscaleError> {
        let bind = self.bind_addr.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("bind address required".into()))
        })?;
        let upstream = self.upstream.ok_or_else(|| {
            RailscaleError::from(ErrorKind::RoutingFailed("upstream address required".into()))
        })?;
        let source = TcpSource::bind(&bind).await?;
        Ok(ForwardTcp {
            source,
            upstream,
            drain_timeout: self.drain_timeout,
        })
    }
}
