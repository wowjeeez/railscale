use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Accept,
    Parse,
    Routing,
    Forward,
    Relay,
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accept => write!(f, "accept"),
            Self::Parse => write!(f, "parse"),
            Self::Routing => write!(f, "routing"),
            Self::Forward => write!(f, "forward"),
            Self::Relay => write!(f, "relay"),
        }
    }
}

pub enum ErrorKind {
    Io(std::io::Error),
    Parse(String),
    RoutingFailed(String),
    ConnectionClosed,
    NoRoutingFrame,
    BufferLimitExceeded,
}

pub struct RailscaleError {
    pub kind: ErrorKind,
    pub phase: Option<Phase>,
}

impl RailscaleError {
    pub fn in_phase(mut self, phase: Phase) -> Self {
        self.phase = Some(phase);
        self
    }
}

impl fmt::Display for RailscaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(phase) = &self.phase {
            write!(f, "[{phase}] ")?;
        }
        match &self.kind {
            ErrorKind::Io(e) => write!(f, "io: {e}"),
            ErrorKind::Parse(msg) => write!(f, "parse: {msg}"),
            ErrorKind::RoutingFailed(msg) => write!(f, "routing failed: {msg}"),
            ErrorKind::ConnectionClosed => write!(f, "connection closed"),
            ErrorKind::NoRoutingFrame => write!(f, "no routing frame received"),
            ErrorKind::BufferLimitExceeded => write!(f, "buffer limit exceeded"),
        }
    }
}

impl fmt::Debug for RailscaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for RailscaleError {}

impl From<std::io::Error> for RailscaleError {
    fn from(e: std::io::Error) -> Self {
        Self { kind: ErrorKind::Io(e), phase: None }
    }
}

impl From<ErrorKind> for RailscaleError {
    fn from(kind: ErrorKind) -> Self {
        Self { kind, phase: None }
    }
}
