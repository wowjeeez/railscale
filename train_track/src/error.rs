use std::fmt;

pub enum RailscaleError {
    Io(std::io::Error),
    Parse(String),
    RoutingFailed(String),
    ConnectionClosed,
    NoRoutingFrame,
}

impl fmt::Display for RailscaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Parse(msg) => write!(f, "parse: {msg}"),
            Self::RoutingFailed(msg) => write!(f, "routing failed: {msg}"),
            Self::ConnectionClosed => write!(f, "connection closed"),
            Self::NoRoutingFrame => write!(f, "no routing frame received"),
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
        Self::Io(e)
    }
}
