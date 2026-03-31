use bytes::Bytes;
use train_track::{MatchAtom, DerivationFormula, DeriverSession, DerivedEffect};
use carriage::http_v1::derive::{
    Matcher, HttpVersion, BodyFramingMode, ConnectionMode,
    VersionFormula, BodyFramingFormula, ConnectionFormula,
    HttpDerivationInput,
};
use carriage::http_v1::HttpPhase;

#[test]
fn header_name_matches_case_insensitive() {
    let m = Matcher::HeaderName(b"Content-Length");
    assert_eq!(m.try_match(b"Content-Length: 42\r\n"), Some(Bytes::from_static(b"42")));
    assert_eq!(m.try_match(b"content-length: 100\r\n"), Some(Bytes::from_static(b"100")));
}

#[test]
fn header_name_no_match() {
    let m = Matcher::HeaderName(b"Content-Length");
    assert_eq!(m.try_match(b"Host: example.com\r\n"), None);
}

#[test]
fn header_name_trims_both_sides() {
    let m = Matcher::HeaderName(b"Transfer-Encoding");
    assert_eq!(m.try_match(b"Transfer-Encoding:  chunked  \r\n"), Some(Bytes::from_static(b"chunked")));
    assert_eq!(m.try_match(b"Transfer-Encoding: \t close \t \r\n"), Some(Bytes::from_static(b"close")));
}

#[test]
fn header_name_empty_value() {
    let m = Matcher::HeaderName(b"Content-Length");
    assert_eq!(m.try_match(b"Content-Length:\r\n"), Some(Bytes::from_static(b"")));
    assert_eq!(m.try_match(b"Content-Length:   \r\n"), Some(Bytes::from_static(b"")));
}

#[test]
fn request_line_version() {
    let m = Matcher::RequestLineVersion;
    assert_eq!(m.try_match(b"GET / HTTP/1.1\r\n"), Some(Bytes::from_static(b"HTTP/1.1")));
    assert_eq!(m.try_match(b"POST /api HTTP/1.0\r\n"), Some(Bytes::from_static(b"HTTP/1.0")));
}

#[test]
fn request_line_method() {
    let m = Matcher::RequestLineMethod;
    assert_eq!(m.try_match(b"GET / HTTP/1.1\r\n"), Some(Bytes::from_static(b"GET")));
    assert_eq!(m.try_match(b"POST /api HTTP/1.1\r\n"), Some(Bytes::from_static(b"POST")));
}

#[test]
fn request_line_uri() {
    let m = Matcher::RequestLineUri;
    assert_eq!(m.try_match(b"GET /foo/bar HTTP/1.1\r\n"), Some(Bytes::from_static(b"/foo/bar")));
}

#[test]
fn status_code() {
    let m = Matcher::StatusCode;
    assert_eq!(m.try_match(b"HTTP/1.1 200 OK\r\n"), Some(Bytes::from_static(b"200")));
    assert_eq!(m.try_match(b"HTTP/1.1 404 Not Found\r\n"), Some(Bytes::from_static(b"404")));
}

#[test]
fn matcher_dedup_via_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(Matcher::HeaderName(b"Content-Length"));
    set.insert(Matcher::HeaderName(b"Content-Length"));
    set.insert(Matcher::RequestLineVersion);
    assert_eq!(set.len(), 2);
}

#[test]
fn header_name_no_match_on_request_line() {
    let m = Matcher::HeaderName(b"GET");
    assert_eq!(m.try_match(b"GET / HTTP/1.1\r\n"), None);
}

#[test]
fn version_formula_http11() {
    let matched = vec![Some(Bytes::from_static(b"HTTP/1.1"))];
    assert_eq!(VersionFormula::resolve(&matched), HttpVersion::Http11);
}

#[test]
fn version_formula_http10() {
    let matched = vec![Some(Bytes::from_static(b"HTTP/1.0"))];
    assert_eq!(VersionFormula::resolve(&matched), HttpVersion::Http10);
}

#[test]
fn version_formula_missing() {
    let matched = vec![None];
    assert_eq!(VersionFormula::resolve(&matched), HttpVersion::Http11);
}

#[test]
fn body_framing_content_length() {
    let matched = vec![
        Some(Bytes::from_static(b"42")),
        None,
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Fixed(42));
}

#[test]
fn body_framing_chunked() {
    let matched = vec![
        None,
        Some(Bytes::from_static(b"chunked")),
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Chunked);
}

#[test]
fn body_framing_chunked_compound() {
    let matched = vec![
        None,
        Some(Bytes::from_static(b"gzip, chunked")),
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Chunked);
}

#[test]
fn body_framing_chunked_compound_leading() {
    let matched = vec![
        None,
        Some(Bytes::from_static(b"chunked, gzip")),
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Chunked);
}

#[test]
fn body_framing_te_not_chunked() {
    let matched = vec![
        Some(Bytes::from_static(b"100")),
        Some(Bytes::from_static(b"gzip")),
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Fixed(100));
}

#[test]
fn body_framing_http10_no_length() {
    let matched = vec![
        None,
        None,
        Some(Bytes::from_static(b"HTTP/1.0")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::UntilClose);
}

#[test]
fn body_framing_http11_no_length() {
    let matched = vec![
        None,
        None,
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::None);
}

#[test]
fn body_framing_malformed_content_length() {
    let matched = vec![
        Some(Bytes::from_static(b"abc")),
        None,
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Invalid);
}

#[test]
fn body_framing_negative_content_length() {
    let matched = vec![
        Some(Bytes::from_static(b"-1")),
        None,
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Invalid);
}

#[test]
fn body_framing_overflow_content_length() {
    let matched = vec![
        Some(Bytes::from_static(b"99999999999999999999")),
        None,
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Invalid);
}

#[test]
fn body_framing_empty_content_length() {
    let matched = vec![
        Some(Bytes::from_static(b"")),
        None,
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(BodyFramingFormula::resolve(&matched), BodyFramingMode::Invalid);
}

#[test]
fn connection_explicit_close() {
    let matched = vec![
        Some(Bytes::from_static(b"close")),
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(ConnectionFormula::resolve(&matched), ConnectionMode::Close);
}

#[test]
fn connection_explicit_keep_alive() {
    let matched = vec![
        Some(Bytes::from_static(b"keep-alive")),
        Some(Bytes::from_static(b"HTTP/1.0")),
    ];
    assert_eq!(ConnectionFormula::resolve(&matched), ConnectionMode::KeepAlive);
}

#[test]
fn connection_http11_default() {
    let matched = vec![
        None,
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(ConnectionFormula::resolve(&matched), ConnectionMode::KeepAlive);
}

#[test]
fn connection_http10_default() {
    let matched = vec![
        None,
        Some(Bytes::from_static(b"HTTP/1.0")),
    ];
    assert_eq!(ConnectionFormula::resolve(&matched), ConnectionMode::Close);
}

#[test]
fn connection_multi_value_with_close() {
    let matched = vec![
        Some(Bytes::from_static(b"keep-alive, close")),
        Some(Bytes::from_static(b"HTTP/1.1")),
    ];
    assert_eq!(ConnectionFormula::resolve(&matched), ConnectionMode::Close);
}

#[test]
fn connection_multi_value_with_keep_alive() {
    let matched = vec![
        Some(Bytes::from_static(b"upgrade, keep-alive")),
        Some(Bytes::from_static(b"HTTP/1.0")),
    ];
    assert_eq!(ConnectionFormula::resolve(&matched), ConnectionMode::KeepAlive);
}

#[test]
fn deriver_session_deduplicates_matchers() {
    let matchers = vec![
        Matcher::RequestLineVersion,
        Matcher::HeaderName(b"Content-Length"),
        Matcher::RequestLineVersion,
        Matcher::HeaderName(b"Connection"),
        Matcher::HeaderName(b"Content-Length"),
    ];
    let session = DeriverSession::new(matchers);
    assert_eq!(session.matcher_count(), 3);
}

#[test]
fn full_derivation_http11_with_content_length() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"GET /index.html HTTP/1.1\r\n");
    session.feed(&HttpPhase::Header, b"Host: example.com\r\n");
    session.feed(&HttpPhase::Header, b"Content-Length: 256\r\n");
    session.feed(&HttpPhase::Header, b"Accept: text/html\r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert_eq!(input.version, HttpVersion::Http11);
    assert_eq!(input.body_framing, BodyFramingMode::Fixed(256));
    assert_eq!(input.connection, ConnectionMode::KeepAlive);
    assert!(!input.has_conflicts);
    assert!(!input.cl_te_conflict);
}

#[test]
fn full_derivation_http10_chunked_close() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"POST /upload HTTP/1.0\r\n");
    session.feed(&HttpPhase::Header, b"Transfer-Encoding: chunked\r\n");
    session.feed(&HttpPhase::Header, b"Connection: close\r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert_eq!(input.version, HttpVersion::Http10);
    assert_eq!(input.body_framing, BodyFramingMode::Chunked);
    assert_eq!(input.connection, ConnectionMode::Close);
}

#[test]
fn full_derivation_http10_no_headers() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"GET / HTTP/1.0\r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert_eq!(input.version, HttpVersion::Http10);
    assert_eq!(input.body_framing, BodyFramingMode::UntilClose);
    assert_eq!(input.connection, ConnectionMode::Close);
}

#[test]
fn full_derivation_http11_keep_alive_no_body() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"GET / HTTP/1.1\r\n");
    session.feed(&HttpPhase::Header, b"Host: example.com\r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert_eq!(input.version, HttpVersion::Http11);
    assert_eq!(input.body_framing, BodyFramingMode::None);
    assert_eq!(input.connection, ConnectionMode::KeepAlive);
}

#[test]
fn cl_te_conflict_detected() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"POST / HTTP/1.1\r\n");
    session.feed(&HttpPhase::Header, b"Content-Length: 10\r\n");
    session.feed(&HttpPhase::Header, b"Transfer-Encoding: chunked\r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert!(input.cl_te_conflict);
    assert_eq!(input.body_framing, BodyFramingMode::Chunked);
}

#[test]
fn duplicate_content_length_conflict() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"POST / HTTP/1.1\r\n");
    session.feed(&HttpPhase::Header, b"Content-Length: 10\r\n");
    session.feed(&HttpPhase::Header, b"Content-Length: 20\r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert!(input.has_conflicts);
}

#[test]
fn duplicate_content_length_same_value_no_conflict() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"POST / HTTP/1.1\r\n");
    session.feed(&HttpPhase::Header, b"Content-Length: 10\r\n");
    session.feed(&HttpPhase::Header, b"Content-Length: 10\r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert!(!input.has_conflicts);
}

#[test]
fn trailing_whitespace_on_chunked_still_matches() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"POST / HTTP/1.1\r\n");
    session.feed(&HttpPhase::Header, b"Transfer-Encoding: chunked  \r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert_eq!(input.body_framing, BodyFramingMode::Chunked);
}

#[test]
fn trailing_whitespace_on_connection_close_still_matches() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"GET / HTTP/1.1\r\n");
    session.feed(&HttpPhase::Header, b"Connection: close   \r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    assert_eq!(input.connection, ConnectionMode::Close);
}

struct MockDestination {
    version: Option<HttpVersion>,
    body_framing: Option<BodyFramingMode>,
    connection: Option<ConnectionMode>,
}

impl MockDestination {
    fn new() -> Self {
        Self { version: None, body_framing: None, connection: None }
    }
}

impl DerivedEffect<HttpVersion> for MockDestination {
    fn apply_effect(&mut self, effect: HttpVersion) {
        self.version = Some(effect);
    }
}

impl DerivedEffect<BodyFramingMode> for MockDestination {
    fn apply_effect(&mut self, effect: BodyFramingMode) {
        self.body_framing = Some(effect);
    }
}

impl DerivedEffect<ConnectionMode> for MockDestination {
    fn apply_effect(&mut self, effect: ConnectionMode) {
        self.connection = Some(effect);
    }
}

#[test]
fn apply_effects_to_destination() {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, b"GET / HTTP/1.1\r\n");
    session.feed(&HttpPhase::Header, b"Content-Length: 100\r\n");
    let input = HttpDerivationInput::resolve_all(&session);
    let mut dest = MockDestination::new();
    input.apply_to(&mut dest);
    assert_eq!(dest.version, Some(HttpVersion::Http11));
    assert_eq!(dest.body_framing, Some(BodyFramingMode::Fixed(100)));
    assert_eq!(dest.connection, Some(ConnectionMode::KeepAlive));
}
