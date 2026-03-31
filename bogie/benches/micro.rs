use bytes::BytesMut;
use criterion::{criterion_group, criterion_main, Criterion, black_box};
use tokio_util::codec::Decoder;
use carriage::http_v1::HttpStreamingCodec;
use train_track::MatchAtom;
use carriage::http_v1::derive::{Matcher, HttpDerivationInput};
use carriage::http_v1::HttpPhase;
use train_track::DeriverSession;
use std::io::Cursor;
use tokio::runtime::Runtime;
use tokio_stream::StreamExt;
use trezorcarriage::TlsParser;
use train_track::{Frame, FrameParser, ParsedData};

fn bench_codec_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("codec_decode");

    let simple_get = b"GET / HTTP/1.1\r\nHost: example.com\r\nAccept: */*\r\nConnection: close\r\n\r\n";
    group.bench_function("decode_simple_get", |b| {
        b.iter(|| {
            let mut codec = HttpStreamingCodec::new(vec![]);
            let mut buf = BytesMut::from(&simple_get[..]);
            while let Ok(Some(_)) = codec.decode(&mut buf) {
                if codec.headers_done() { break; }
            }
        })
    });

    let mut large_msg = b"GET / HTTP/1.1\r\n".to_vec();
    for i in 0..50 {
        large_msg.extend_from_slice(format!("X-Header-{i}: value-{i}\r\n").as_bytes());
    }
    large_msg.extend_from_slice(b"\r\n");
    group.bench_function("decode_large_headers", |b| {
        b.iter(|| {
            let mut codec = HttpStreamingCodec::new(vec![]);
            let mut buf = BytesMut::from(&large_msg[..]);
            while let Ok(Some(_)) = codec.decode(&mut buf) {
                if codec.headers_done() { break; }
            }
        })
    });

    let simple_get_vec = simple_get.to_vec();
    group.bench_function("decode_fragmented", |b| {
        b.iter(|| {
            let mut codec = HttpStreamingCodec::new(vec![]);
            let mut buf = BytesMut::new();
            for chunk in simple_get_vec.chunks(64) {
                buf.extend_from_slice(chunk);
                while let Ok(Some(_)) = codec.decode(&mut buf) {
                    if codec.headers_done() { return; }
                }
            }
        })
    });

    group.finish();
}

fn bench_matcher(c: &mut Criterion) {
    let mut group = c.benchmark_group("matcher");

    let m = Matcher::HeaderName(b"Content-Length");
    group.bench_function("header_name_match", |b| {
        b.iter(|| {
            black_box(m.try_match(black_box(b"Content-Length: 42\r\n")));
        })
    });

    group.bench_function("header_name_miss", |b| {
        b.iter(|| {
            black_box(m.try_match(black_box(b"Host: example.com\r\n")));
        })
    });

    let v = Matcher::RequestLineVersion;
    group.bench_function("request_line_version", |b| {
        b.iter(|| {
            black_box(v.try_match(black_box(b"GET / HTTP/1.1\r\n")));
        })
    });

    group.finish();
}

fn bench_derive(c: &mut Criterion) {
    let mut group = c.benchmark_group("derive");

    let headers: &[&[u8]] = &[
        b"Host: example.com\r\n",
        b"Content-Length: 256\r\n",
        b"Accept: text/html\r\n",
        b"Connection: keep-alive\r\n",
        b"User-Agent: bench/1.0\r\n",
    ];

    group.bench_function("session_feed_and_resolve", |b| {
        b.iter(|| {
            let mut session = DeriverSession::new(HttpDerivationInput::all_matchers());
            session.feed(&HttpPhase::RequestLine, b"GET / HTTP/1.1\r\n");
            for h in headers {
                session.feed(&HttpPhase::Header, *h);
            }
            black_box(HttpDerivationInput::resolve_all(&session));
        })
    });

    group.finish();
}

fn build_bench_client_hello(hostname: &[u8]) -> Vec<u8> {
    let name_len = hostname.len();
    let server_name_list_len = 1 + 2 + name_len;
    let sni_data_len = 2 + server_name_list_len;
    let client_hello_body_len =
        2 + 32 + 1 + 2 + 2 + 1 + 1 + 2 + 2 + 2 + sni_data_len;

    let mut buf = Vec::new();
    buf.push(0x16);
    buf.extend_from_slice(&[0x03, 0x01]);
    let record_payload_len = 4 + client_hello_body_len;
    buf.push((record_payload_len >> 8) as u8);
    buf.push((record_payload_len & 0xff) as u8);

    buf.push(0x01);
    buf.push(((client_hello_body_len >> 16) & 0xff) as u8);
    buf.push(((client_hello_body_len >> 8) & 0xff) as u8);
    buf.push((client_hello_body_len & 0xff) as u8);

    buf.extend_from_slice(&[0x03, 0x03]);
    buf.extend_from_slice(&[0u8; 32]);
    buf.push(0x00);
    buf.extend_from_slice(&[0x00, 0x02, 0x00, 0x9c]);
    buf.push(0x01);
    buf.push(0x00);

    let ext_total_len = 2 + 2 + sni_data_len;
    buf.push((ext_total_len >> 8) as u8);
    buf.push((ext_total_len & 0xff) as u8);

    buf.extend_from_slice(&[0x00, 0x00]);
    buf.push((sni_data_len >> 8) as u8);
    buf.push((sni_data_len & 0xff) as u8);
    buf.push((server_name_list_len >> 8) as u8);
    buf.push((server_name_list_len & 0xff) as u8);
    buf.push(0x00);
    buf.push((name_len >> 8) as u8);
    buf.push((name_len & 0xff) as u8);
    buf.extend_from_slice(hostname);

    buf
}

fn build_bench_client_hello_deep_extensions(hostname: &[u8], extra_count: usize) -> Vec<u8> {
    let name_len = hostname.len();
    let server_name_list_len = 1 + 2 + name_len;
    let sni_data_len = 2 + server_name_list_len;

    let mut extensions = Vec::new();
    for i in 0..extra_count {
        let ext_type = (0xff01u16 + i as u16).to_be_bytes();
        extensions.extend_from_slice(&ext_type);
        extensions.extend_from_slice(&[0x00, 0x02, 0xde, 0xad]);
    }
    extensions.extend_from_slice(&[0x00, 0x00]);
    extensions.push((sni_data_len >> 8) as u8);
    extensions.push((sni_data_len & 0xff) as u8);
    extensions.push((server_name_list_len >> 8) as u8);
    extensions.push((server_name_list_len & 0xff) as u8);
    extensions.push(0x00);
    extensions.push((name_len >> 8) as u8);
    extensions.push((name_len & 0xff) as u8);
    extensions.extend_from_slice(hostname);

    let client_hello_body_len = 2 + 32 + 1 + 2 + 2 + 1 + 1 + 2 + extensions.len();

    let mut buf = Vec::new();
    buf.push(0x16);
    buf.extend_from_slice(&[0x03, 0x01]);
    let record_payload_len = 4 + client_hello_body_len;
    buf.push((record_payload_len >> 8) as u8);
    buf.push((record_payload_len & 0xff) as u8);

    buf.push(0x01);
    buf.push(((client_hello_body_len >> 16) & 0xff) as u8);
    buf.push(((client_hello_body_len >> 8) & 0xff) as u8);
    buf.push((client_hello_body_len & 0xff) as u8);

    buf.extend_from_slice(&[0x03, 0x03]);
    buf.extend_from_slice(&[0u8; 32]);
    buf.push(0x00);
    buf.extend_from_slice(&[0x00, 0x02, 0x00, 0x9c]);
    buf.push(0x01);
    buf.push(0x00);
    buf.push((extensions.len() >> 8) as u8);
    buf.push((extensions.len() & 0xff) as u8);
    buf.extend_from_slice(&extensions);

    buf
}

fn bench_tls_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("tls_parser");
    let rt = Runtime::new().unwrap();

    let mut single_record = vec![23u8, 0x03, 0x03, 0x04, 0x00];
    single_record.extend_from_slice(&vec![0xAB; 1024]);

    group.bench_function("parse_single_record", |b| {
        b.iter(|| {
            rt.block_on(async {
                let cursor = Cursor::new(black_box(single_record.clone()));
                let mut parser = TlsParser::new();
                let stream = parser.parse(cursor);
                tokio::pin!(stream);
                while let Some(Ok(_)) = stream.next().await {}
            });
        })
    });

    let client_hello = build_bench_client_hello(b"example.com");
    group.bench_function("parse_client_hello_sni", |b| {
        b.iter(|| {
            rt.block_on(async {
                let cursor = Cursor::new(black_box(client_hello.clone()));
                let mut parser = TlsParser::new();
                let stream = parser.parse(cursor);
                tokio::pin!(stream);
                if let Some(Ok(ParsedData::Parsed(frame))) = stream.next().await {
                    black_box(frame.routing_key());
                }
            });
        })
    });

    let mut ten_records = Vec::new();
    for _ in 0..10 {
        ten_records.push(23u8);
        ten_records.extend_from_slice(&[0x03, 0x03, 0x00, 0x80]);
        ten_records.extend_from_slice(&vec![0xCC; 128]);
    }
    group.bench_function("parse_stream_10_records", |b| {
        b.iter(|| {
            rt.block_on(async {
                let cursor = Cursor::new(black_box(ten_records.clone()));
                let mut parser = TlsParser::new();
                let stream = parser.parse(cursor);
                tokio::pin!(stream);
                while let Some(Ok(_)) = stream.next().await {}
            });
        })
    });

    let deep_hello = build_bench_client_hello_deep_extensions(b"example.com", 20);
    group.bench_function("sni_extraction_deep_extensions", |b| {
        b.iter(|| {
            rt.block_on(async {
                let cursor = Cursor::new(black_box(deep_hello.clone()));
                let mut parser = TlsParser::new();
                let stream = parser.parse(cursor);
                tokio::pin!(stream);
                if let Some(Ok(ParsedData::Parsed(frame))) = stream.next().await {
                    black_box(frame.routing_key());
                }
            });
        })
    });

    group.finish();
}

criterion_group!(benches, bench_codec_decode, bench_matcher, bench_derive, bench_tls_parser);
criterion_main!(benches);
