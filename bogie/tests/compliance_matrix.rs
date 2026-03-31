use carriage::http_v1::derive::{
    HttpVersion, BodyFramingMode, ConnectionMode,
    HttpDerivationInput,
};
use carriage::http_v1::HttpPhase;
use train_track::DeriverSession;

struct ComplianceCase {
    name: &'static str,
    rfc_section: &'static str,
    request_line: &'static [u8],
    headers: &'static [&'static [u8]],
    expected_version: HttpVersion,
    expected_body_framing: BodyFramingMode,
    expected_connection: ConnectionMode,
    expected_cl_te_conflict: bool,
    expected_has_conflicts: bool,
}

fn run_case(case: &ComplianceCase) {
    let _ = case.rfc_section;
    let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
    session.feed(&HttpPhase::RequestLine, case.request_line);
    for header in case.headers {
        session.feed(&HttpPhase::Header, header);
    }
    let input = HttpDerivationInput::resolve_all(&session);
    assert_eq!(input.version, case.expected_version, "{}: version", case.name);
    assert_eq!(input.body_framing, case.expected_body_framing, "{}: body_framing", case.name);
    assert_eq!(input.connection, case.expected_connection, "{}: connection", case.name);
    assert_eq!(input.cl_te_conflict, case.expected_cl_te_conflict, "{}: cl_te_conflict", case.name);
    assert_eq!(input.has_conflicts, case.expected_has_conflicts, "{}: has_conflicts", case.name);
}

#[test]
fn rfc7230_3_3_3_cl_present_no_te() {
    run_case(&ComplianceCase {
        name: "CL present, no TE",
        rfc_section: "3.3.3",
        request_line: b"GET / HTTP/1.1\r\n",
        headers: &[b"Content-Length: 42\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Fixed(42),
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_te_chunked_no_cl() {
    run_case(&ComplianceCase {
        name: "TE chunked, no CL",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Transfer-Encoding: chunked\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Chunked,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_both_cl_and_te() {
    run_case(&ComplianceCase {
        name: "Both CL and TE (current behavior)",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[
            b"Content-Length: 100\r\n",
            b"Transfer-Encoding: chunked\r\n",
        ],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Chunked,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: true,
        expected_has_conflicts: false,
    });
}

#[test]
#[ignore = "RFC 7230\u{00a7}3.3.3: CL+TE should be protocol error"]
fn rfc7230_3_3_3_cl_te_should_reject() {
    run_case(&ComplianceCase {
        name: "CL+TE should reject",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[
            b"Content-Length: 100\r\n",
            b"Transfer-Encoding: chunked\r\n",
        ],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Invalid,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: true,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_no_cl_no_te_http10() {
    run_case(&ComplianceCase {
        name: "HTTP/1.0, no headers",
        rfc_section: "3.3.3",
        request_line: b"GET / HTTP/1.0\r\n",
        headers: &[],
        expected_version: HttpVersion::Http10,
        expected_body_framing: BodyFramingMode::UntilClose,
        expected_connection: ConnectionMode::Close,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_no_cl_no_te_http11() {
    run_case(&ComplianceCase {
        name: "HTTP/1.1, Host only",
        rfc_section: "3.3.3",
        request_line: b"GET / HTTP/1.1\r\n",
        headers: &[b"Host: example.com\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::None,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_multiple_cl_same() {
    run_case(&ComplianceCase {
        name: "Two CL: 10, CL: 10",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[
            b"Content-Length: 10\r\n",
            b"Content-Length: 10\r\n",
        ],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Fixed(10),
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_multiple_cl_different() {
    run_case(&ComplianceCase {
        name: "CL: 10, CL: 20",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[
            b"Content-Length: 10\r\n",
            b"Content-Length: 20\r\n",
        ],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Fixed(10),
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: true,
    });
}

#[test]
fn rfc7230_3_3_3_malformed_cl_non_numeric() {
    run_case(&ComplianceCase {
        name: "CL: abc",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Content-Length: abc\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Invalid,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_negative_cl() {
    run_case(&ComplianceCase {
        name: "CL: -1",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Content-Length: -1\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Invalid,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_overflow_cl() {
    run_case(&ComplianceCase {
        name: "CL: 99999999999999999999",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Content-Length: 99999999999999999999\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Invalid,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_3_empty_cl() {
    run_case(&ComplianceCase {
        name: "CL: (empty)",
        rfc_section: "3.3.3",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Content-Length: \r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Invalid,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_2_cl_with_ows() {
    run_case(&ComplianceCase {
        name: "CL with whitespace",
        rfc_section: "3.3.2",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Content-Length:   42  \r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Fixed(42),
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_2_cl_leading_zeros() {
    run_case(&ComplianceCase {
        name: "CL: 0042",
        rfc_section: "3.3.2",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Content-Length: 0042\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Fixed(42),
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_1_te_single_chunked() {
    run_case(&ComplianceCase {
        name: "TE: chunked",
        rfc_section: "3.3.1",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Transfer-Encoding: chunked\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Chunked,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_1_te_compound_trailing() {
    run_case(&ComplianceCase {
        name: "TE: gzip, chunked",
        rfc_section: "3.3.1",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Transfer-Encoding: gzip, chunked\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Chunked,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_1_te_compound_leading() {
    run_case(&ComplianceCase {
        name: "TE: chunked, gzip",
        rfc_section: "3.3.1",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Transfer-Encoding: chunked, gzip\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Chunked,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_1_te_no_chunked() {
    run_case(&ComplianceCase {
        name: "TE: gzip + CL: 100",
        rfc_section: "3.3.1",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[
            b"Transfer-Encoding: gzip\r\n",
            b"Content-Length: 100\r\n",
        ],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Fixed(100),
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: true,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_3_3_1_te_case_insensitive() {
    run_case(&ComplianceCase {
        name: "TE: CHUNKED",
        rfc_section: "3.3.1",
        request_line: b"POST / HTTP/1.1\r\n",
        headers: &[b"Transfer-Encoding: CHUNKED\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Chunked,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_6_1_connection_close() {
    run_case(&ComplianceCase {
        name: "Connection: close",
        rfc_section: "6.1",
        request_line: b"GET / HTTP/1.1\r\n",
        headers: &[b"Connection: close\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::None,
        expected_connection: ConnectionMode::Close,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_6_1_connection_keepalive() {
    run_case(&ComplianceCase {
        name: "Connection: keep-alive on HTTP/1.0",
        rfc_section: "6.1",
        request_line: b"GET / HTTP/1.0\r\n",
        headers: &[b"Connection: keep-alive\r\n"],
        expected_version: HttpVersion::Http10,
        expected_body_framing: BodyFramingMode::UntilClose,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_6_1_connection_multi_with_close() {
    run_case(&ComplianceCase {
        name: "Connection: keep-alive, close",
        rfc_section: "6.1",
        request_line: b"GET / HTTP/1.1\r\n",
        headers: &[b"Connection: keep-alive, close\r\n"],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::None,
        expected_connection: ConnectionMode::Close,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_6_1_connection_multi_with_keepalive() {
    run_case(&ComplianceCase {
        name: "Connection: upgrade, keep-alive on HTTP/1.0",
        rfc_section: "6.1",
        request_line: b"GET / HTTP/1.0\r\n",
        headers: &[b"Connection: upgrade, keep-alive\r\n"],
        expected_version: HttpVersion::Http10,
        expected_body_framing: BodyFramingMode::UntilClose,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_6_1_connection_default_http11() {
    run_case(&ComplianceCase {
        name: "HTTP/1.1 default connection",
        rfc_section: "6.1",
        request_line: b"GET / HTTP/1.1\r\n",
        headers: &[],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::None,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_6_1_connection_default_http10() {
    run_case(&ComplianceCase {
        name: "HTTP/1.0 default connection",
        rfc_section: "6.1",
        request_line: b"GET / HTTP/1.0\r\n",
        headers: &[],
        expected_version: HttpVersion::Http10,
        expected_body_framing: BodyFramingMode::UntilClose,
        expected_connection: ConnectionMode::Close,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_2_6_http11() {
    run_case(&ComplianceCase {
        name: "HTTP/1.1 version",
        rfc_section: "2.6",
        request_line: b"GET / HTTP/1.1\r\n",
        headers: &[],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::None,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
fn rfc7230_2_6_http10() {
    run_case(&ComplianceCase {
        name: "HTTP/1.0 version",
        rfc_section: "2.6",
        request_line: b"GET / HTTP/1.0\r\n",
        headers: &[],
        expected_version: HttpVersion::Http10,
        expected_body_framing: BodyFramingMode::UntilClose,
        expected_connection: ConnectionMode::Close,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}

#[test]
#[ignore = "RFC 7230\u{00a7}2.6: unknown version should be rejected"]
fn rfc7230_2_6_unknown_version_should_reject() {
    run_case(&ComplianceCase {
        name: "HTTP/2.0 unknown version",
        rfc_section: "2.6",
        request_line: b"GET / HTTP/2.0\r\n",
        headers: &[],
        expected_version: HttpVersion::Http11,
        expected_body_framing: BodyFramingMode::Invalid,
        expected_connection: ConnectionMode::KeepAlive,
        expected_cl_te_conflict: false,
        expected_has_conflicts: false,
    });
}
