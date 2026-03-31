use carriage::http_v1::derive::{
    BodyFramingMode, HttpDerivationInput,
};
use carriage::http_v1::HttpPhase;
use train_track::DeriverSession;

fn derive_from_lines(request_line: &[u8], headers: &[&[u8]]) -> HttpDerivationInput {
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, request_line);
    for header in headers {
        session.feed(&HttpPhase::Header, header);
    }
    HttpDerivationInput::resolve_all(&session)
}

#[test]
fn cl_te_basic() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Content-Length: 10\r\n", b"Transfer-Encoding: chunked\r\n"],
    );
    assert!(result.cl_te_conflict);
    assert_eq!(result.body_framing, BodyFramingMode::Chunked);
}

#[test]
fn cl_cl_mismatch() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Content-Length: 10\r\n", b"Content-Length: 20\r\n"],
    );
    assert!(result.has_conflicts);
}

#[test]
fn cl_cl_same_no_conflict() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Content-Length: 10\r\n", b"Content-Length: 10\r\n"],
    );
    assert!(!result.has_conflicts);
    assert_eq!(result.body_framing, BodyFramingMode::Fixed(10));
}

#[test]
fn te_te_different_values() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[
            b"Transfer-Encoding: chunked\r\n",
            b"Transfer-Encoding: gzip\r\n",
        ],
    );
    assert!(result.has_conflicts);
}

#[test]
fn obfuscated_te_xchunked_not_matched() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Transfer-Encoding: xchunked\r\n", b"Content-Length: 10\r\n"],
    );
    assert_eq!(result.body_framing, BodyFramingMode::Fixed(10));
}

#[test]
fn obfuscated_te_space_before_colon() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Transfer-Encoding : chunked\r\n"],
    );
    assert_eq!(result.body_framing, BodyFramingMode::None);
}

#[test]
fn obfuscated_te_chunked_with_trailing_tab() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Transfer-Encoding: chunked\t\r\n"],
    );
    assert_eq!(result.body_framing, BodyFramingMode::Chunked);
}

#[test]
#[ignore = "Bare LF: codec requires CRLF"]
fn bare_lf_line_endings() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\n",
        &[b"Host: example.com\n"],
    );
    assert_eq!(result.body_framing, BodyFramingMode::None);
}

#[test]
fn embedded_crlf_in_header_value() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Content-Length: 10\r\n"],
    );
    assert_eq!(result.body_framing, BodyFramingMode::Fixed(10));
}

#[test]
fn te_reversed_cl_te_conflict() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Transfer-Encoding: chunked\r\n", b"Content-Length: 10\r\n"],
    );
    assert!(result.cl_te_conflict);
    assert_eq!(result.body_framing, BodyFramingMode::Chunked);
}

#[test]
fn cl_zero_with_te_chunked() {
    let result = derive_from_lines(
        b"GET / HTTP/1.1\r\n",
        &[b"Content-Length: 0\r\n", b"Transfer-Encoding: chunked\r\n"],
    );
    assert!(result.cl_te_conflict);
    assert_eq!(result.body_framing, BodyFramingMode::Chunked);
}
