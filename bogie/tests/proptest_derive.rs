use proptest::prelude::*;
use train_track::DeriverSession;
use carriage::http_v1::derive::{
    Matcher, HttpVersion, BodyFramingMode, ConnectionMode,
    HttpDerivationInput,
};
use carriage::http_v1::HttpPhase;
use bogie::generators::*;

proptest! {
    #[test]
    fn version_matches_input(line in arb_request_line()) {
        let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
        session.feed(&HttpPhase::RequestLine, &line);
        let input = HttpDerivationInput::resolve_all(&session);
        if line.windows(8).any(|w| w == b"HTTP/1.0") {
            prop_assert_eq!(input.version, HttpVersion::Http10);
        } else {
            prop_assert_eq!(input.version, HttpVersion::Http11);
        }
    }

    #[test]
    fn content_length_without_te_is_fixed(
        line in arb_request_line(),
        cl in 0u64..10_000_000u64,
    ) {
        let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
        session.feed(&HttpPhase::RequestLine, &line);
        let cl_header = format!("Content-Length: {cl}\r\n");
        session.feed(&HttpPhase::Header, cl_header.as_bytes());
        let input = HttpDerivationInput::resolve_all(&session);
        prop_assert_eq!(input.body_framing, BodyFramingMode::Fixed(cl as usize));
        prop_assert!(!input.cl_te_conflict);
    }

    #[test]
    fn chunked_te_always_chunked(
        line in arb_request_line(),
        te in prop_oneof![
            Just(b"Transfer-Encoding: chunked\r\n".to_vec()),
            Just(b"Transfer-Encoding: gzip, chunked\r\n".to_vec()),
            Just(b"Transfer-Encoding: chunked, gzip\r\n".to_vec()),
            Just(b"Transfer-Encoding: CHUNKED\r\n".to_vec()),
        ],
    ) {
        let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
        session.feed(&HttpPhase::RequestLine, &line);
        session.feed(&HttpPhase::Header, &te);
        let input = HttpDerivationInput::resolve_all(&session);
        prop_assert_eq!(input.body_framing, BodyFramingMode::Chunked);
    }

    #[test]
    fn cl_and_te_sets_conflict(
        line in arb_request_line(),
        cl in 0u64..10_000u64,
    ) {
        let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
        session.feed(&HttpPhase::RequestLine, &line);
        let cl_header = format!("Content-Length: {cl}\r\n");
        session.feed(&HttpPhase::Header, cl_header.as_bytes());
        session.feed(&HttpPhase::Header, b"Transfer-Encoding: chunked\r\n");
        let input = HttpDerivationInput::resolve_all(&session);
        prop_assert!(input.cl_te_conflict);
        prop_assert_eq!(input.body_framing, BodyFramingMode::Chunked);
    }

    #[test]
    fn duplicate_cl_different_values_conflicts(
        line in arb_request_line(),
        a in 1u64..10_000u64,
        b in 1u64..10_000u64,
    ) {
        prop_assume!(a != b);
        let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
        session.feed(&HttpPhase::RequestLine, &line);
        session.feed(&HttpPhase::Header, format!("Content-Length: {a}\r\n").as_bytes());
        session.feed(&HttpPhase::Header, format!("Content-Length: {b}\r\n").as_bytes());
        let input = HttpDerivationInput::resolve_all(&session);
        prop_assert!(input.has_conflicts);
    }

    #[test]
    fn duplicate_cl_same_value_no_conflict(
        line in arb_request_line(),
        n in 0u64..10_000u64,
    ) {
        let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
        session.feed(&HttpPhase::RequestLine, &line);
        let header = format!("Content-Length: {n}\r\n");
        session.feed(&HttpPhase::Header, header.as_bytes());
        session.feed(&HttpPhase::Header, header.as_bytes());
        let input = HttpDerivationInput::resolve_all(&session);
        prop_assert!(!input.has_conflicts);
    }

    #[test]
    fn explicit_close_overrides_default(line in arb_request_line()) {
        let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
        session.feed(&HttpPhase::RequestLine, &line);
        session.feed(&HttpPhase::Header, b"Connection: close\r\n");
        let input = HttpDerivationInput::resolve_all(&session);
        prop_assert_eq!(input.connection, ConnectionMode::Close);
    }

    #[test]
    fn explicit_keepalive_overrides_default(line in arb_request_line()) {
        let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
        session.feed(&HttpPhase::RequestLine, &line);
        session.feed(&HttpPhase::Header, b"Connection: keep-alive\r\n");
        let input = HttpDerivationInput::resolve_all(&session);
        prop_assert_eq!(input.connection, ConnectionMode::KeepAlive);
    }

    #[test]
    fn matcher_dedup_never_increases(count in 1usize..20) {
        let mut matchers = Vec::new();
        for _ in 0..count {
            matchers.push(Matcher::RequestLineVersion);
        }
        let session = DeriverSession::new(matchers);
        prop_assert_eq!(session.matcher_count(), 1);
    }
}
