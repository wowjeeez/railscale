use bytes::Bytes;
use std::hash::{Hash, Hasher};
use train_track::{MatchAtom, DerivationFormula, DerivedEffect, DeriverSession};
use crate::http_v1::HttpPhase;
#[cfg(feature = "derive-debug")]
use tracing::debug;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Matcher {
    HeaderName(&'static [u8]),
    RequestLineVersion,
    RequestLineMethod,
    RequestLineUri,
    StatusCode,
}

impl Hash for Matcher {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        if let Matcher::HeaderName(name) = self {
            name.hash(state);
        }
    }
}

fn trim_ows(value: &[u8]) -> &[u8] {
    let start = value.iter().position(|&b| b != b' ' && b != b'\t').unwrap_or(value.len());
    let end = value.iter().rposition(|&b| b != b' ' && b != b'\t').map(|p| p + 1).unwrap_or(start);
    &value[start..end]
}

fn strip_crlf(data: &[u8]) -> &[u8] {
    if data.len() >= 2 && data[data.len() - 2] == b'\r' {
        &data[..data.len() - 2]
    } else {
        data
    }
}

impl MatchAtom for Matcher {
    type Phase = HttpPhase;

    fn phase(&self) -> HttpPhase {
        match self {
            Matcher::HeaderName(_) => HttpPhase::Header,
            Matcher::RequestLineVersion
            | Matcher::RequestLineMethod
            | Matcher::RequestLineUri => HttpPhase::RequestLine,
            Matcher::StatusCode => HttpPhase::StatusLine,
        }
    }

    fn try_match(&self, data: &[u8]) -> Option<Bytes> {
        match self {
            Matcher::HeaderName(name) => {
                let sep = memchr::memchr(b':', data)?;
                let header_name = &data[..sep];
                if !header_name.eq_ignore_ascii_case(name) {
                    return None;
                }
                let raw_value = strip_crlf(&data[sep + 1..]);
                let trimmed = trim_ows(raw_value);
                Some(Bytes::copy_from_slice(trimmed))
            }
            Matcher::RequestLineVersion => {
                let last_space = memchr::memrchr(b' ', data)?;
                let version = strip_crlf(&data[last_space + 1..]);
                Some(Bytes::copy_from_slice(version))
            }
            Matcher::RequestLineMethod => {
                let space = memchr::memchr(b' ', data)?;
                Some(Bytes::copy_from_slice(&data[..space]))
            }
            Matcher::RequestLineUri => {
                let first_space = memchr::memchr(b' ', data)?;
                let rest = &data[first_space + 1..];
                let second_space = memchr::memchr(b' ', rest)?;
                Some(Bytes::copy_from_slice(&rest[..second_space]))
            }
            Matcher::StatusCode => {
                let first_space = memchr::memchr(b' ', data)?;
                let rest = &data[first_space + 1..];
                let end = memchr::memchr(b' ', rest).unwrap_or(rest.len());
                Some(Bytes::copy_from_slice(&rest[..end]))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyFramingMode {
    Fixed(usize),
    Chunked,
    UntilClose,
    None,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionMode {
    KeepAlive,
    Close,
}

pub struct VersionFormula;

impl DerivationFormula for VersionFormula {
    type Matcher = Matcher;
    type Effect = HttpVersion;
    const MATCHERS: &'static [Matcher] = &[Matcher::RequestLineVersion];

    fn resolve(matched: &[Option<Bytes>]) -> HttpVersion {
        match matched.first().and_then(|m| m.as_ref()) {
            Some(v) if v.as_ref() == b"HTTP/1.0" => HttpVersion::Http10,
            _ => HttpVersion::Http11,
        }
    }
}

fn has_chunked_token(value: &[u8]) -> bool {
    value.split(|&b| b == b',')
        .map(trim_ows)
        .any(|token| token.eq_ignore_ascii_case(b"chunked"))
}

pub struct BodyFramingFormula;

impl DerivationFormula for BodyFramingFormula {
    type Matcher = Matcher;
    type Effect = BodyFramingMode;
    const MATCHERS: &'static [Matcher] = &[
        Matcher::HeaderName(b"Content-Length"),
        Matcher::HeaderName(b"Transfer-Encoding"),
        Matcher::RequestLineVersion,
    ];

    fn resolve(matched: &[Option<Bytes>]) -> BodyFramingMode {
        let content_length = matched.first().and_then(|m| m.as_ref());
        let transfer_encoding = matched.get(1).and_then(|m| m.as_ref());
        let version = matched.get(2).and_then(|m| m.as_ref());

        if transfer_encoding.is_some_and(|te| has_chunked_token(te.as_ref())) {
            return BodyFramingMode::Chunked;
        }

        if let Some(cl) = content_length {
            let s = match std::str::from_utf8(cl.as_ref()) {
                Ok(s) => s,
                Err(_) => return BodyFramingMode::Invalid,
            };
            return match s.trim().parse::<usize>() {
                Ok(len) => BodyFramingMode::Fixed(len),
                Err(_) => BodyFramingMode::Invalid,
            };
        }

        let is_10 = version.is_some_and(|v| v.as_ref() == b"HTTP/1.0");
        if is_10 {
            BodyFramingMode::UntilClose
        } else {
            BodyFramingMode::None
        }
    }
}

fn has_connection_token(value: &[u8], token: &[u8]) -> bool {
    value.split(|&b| b == b',')
        .map(trim_ows)
        .any(|t| t.eq_ignore_ascii_case(token))
}

pub struct ConnectionFormula;

impl DerivationFormula for ConnectionFormula {
    type Matcher = Matcher;
    type Effect = ConnectionMode;
    const MATCHERS: &'static [Matcher] = &[
        Matcher::HeaderName(b"Connection"),
        Matcher::RequestLineVersion,
    ];

    fn resolve(matched: &[Option<Bytes>]) -> ConnectionMode {
        let connection = matched.first().and_then(|m| m.as_ref());
        let version = matched.get(1).and_then(|m| m.as_ref());

        if let Some(conn) = connection {
            if has_connection_token(conn.as_ref(), b"close") {
                return ConnectionMode::Close;
            }
            if has_connection_token(conn.as_ref(), b"keep-alive") {
                return ConnectionMode::KeepAlive;
            }
        }

        let is_10 = version.is_some_and(|v| v.as_ref() == b"HTTP/1.0");
        if is_10 {
            ConnectionMode::Close
        } else {
            ConnectionMode::KeepAlive
        }
    }
}

pub struct HttpDerivationInput {
    pub version: HttpVersion,
    pub body_framing: BodyFramingMode,
    pub connection: ConnectionMode,
    pub has_conflicts: bool,
    pub cl_te_conflict: bool,
}

impl HttpDerivationInput {
    pub fn all_matchers() -> Vec<Matcher> {
        let mut all = Vec::new();
        all.extend_from_slice(VersionFormula::MATCHERS);
        all.extend_from_slice(BodyFramingFormula::MATCHERS);
        all.extend_from_slice(ConnectionFormula::MATCHERS);
        all
    }

    pub fn resolve_all(session: &DeriverSession<Matcher>) -> Self {
        let version = VersionFormula::resolve(&session.results_for::<VersionFormula>());
        let body_framing = BodyFramingFormula::resolve(&session.results_for::<BodyFramingFormula>());
        let connection = ConnectionFormula::resolve(&session.results_for::<ConnectionFormula>());

        let has_conflicts =
            session.has_conflicts_for::<BodyFramingFormula>()
            || session.has_conflicts_for::<ConnectionFormula>();

        let bf_results = session.results_for::<BodyFramingFormula>();
        let cl_present = bf_results.first().and_then(|m| m.as_ref()).is_some();
        let te_present = bf_results.get(1).and_then(|m| m.as_ref()).is_some();
        let cl_te_conflict = cl_present && te_present;

        #[cfg(feature = "derive-debug")]
        debug!(
            ?version,
            ?body_framing,
            ?connection,
            has_conflicts,
            cl_te_conflict,
            unique_matchers = session.matcher_count(),
            "derive: all formulas resolved"
        );

        Self { version, body_framing, connection, has_conflicts, cl_te_conflict }
    }

    pub fn apply_to<D>(&self, dest: &mut D)
    where
        D: DerivedEffect<HttpVersion> + DerivedEffect<BodyFramingMode> + DerivedEffect<ConnectionMode>,
    {
        dest.apply_effect(self.version);
        dest.apply_effect(self.body_framing);
        dest.apply_effect(self.connection);
    }
}
