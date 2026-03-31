use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use std::io::Cursor;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::runtime::Runtime;
use tokio_stream::StreamExt;
use train_track::{Frame, FrameParser, ParsedData};
use trezorcarriage::{TlsEncryptedFrame, TlsParser, TlsRecordType};

use bogie::generators::*;

struct SplitReader {
    data: Vec<u8>,
    split: usize,
    pos: usize,
}

impl Unpin for SplitReader {}

impl AsyncRead for SplitReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.pos >= this.data.len() {
            return Poll::Ready(Ok(()));
        }
        let limit = if this.pos < this.split {
            this.split
        } else {
            this.data.len()
        };
        let end = limit.min(this.pos + buf.remaining());
        if end > this.pos {
            buf.put_slice(&this.data[this.pos..end]);
            this.pos = end;
        }
        Poll::Ready(Ok(()))
    }
}

fn parse_tls_frames(reader: impl AsyncRead + Unpin + Send + 'static) -> Vec<Vec<u8>> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let mut parser = TlsParser::new();
        let stream = parser.parse(reader);
        tokio::pin!(stream);
        let mut frames = Vec::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(ParsedData::Parsed(frame)) => frames.push(frame.into_bytes().to_vec()),
                _ => break,
            }
        }
        frames
    })
}

proptest! {
    #[test]
    fn sni_round_trip((hostname, record) in arb_tls_client_hello(arb_hostname())) {
        let frame = TlsEncryptedFrame::new(Bytes::from(record), TlsRecordType::Handshake);
        prop_assert_eq!(frame.routing_key(), Some(hostname.as_bytes()));
    }

    #[test]
    fn parser_split_invariance(records in arb_tls_stream(), split_point in any::<usize>()) {
        let concat: Vec<u8> = records.iter().flatten().cloned().collect();
        if concat.is_empty() {
            return Ok(());
        }
        let split = split_point % concat.len();

        let one_shot = parse_tls_frames(Cursor::new(concat.clone()));
        let split_read = parse_tls_frames(SplitReader {
            data: concat,
            split,
            pos: 0,
        });

        prop_assert_eq!(one_shot, split_read);
    }

    #[test]
    fn record_byte_preservation(rt in arb_tls_record_type(), len in 1usize..256) {
        let record_strategy = arb_tls_record(Just(rt), Just(len));
        let mut runner = proptest::test_runner::TestRunner::default();
        let record = record_strategy.new_tree(&mut runner).unwrap().current();

        let frame = TlsEncryptedFrame::new(
            Bytes::from(record.clone()),
            TlsRecordType::from_u8(rt).unwrap(),
        );
        prop_assert_eq!(frame.as_bytes(), record.as_slice());

        let frame2 = TlsEncryptedFrame::new(
            Bytes::from(record.clone()),
            TlsRecordType::from_u8(rt).unwrap(),
        );
        prop_assert_eq!(&frame2.into_bytes()[..], record.as_slice());
    }

    #[test]
    fn record_type_fidelity(rt_byte in arb_tls_record_type()) {
        let rt = TlsRecordType::from_u8(rt_byte).unwrap();
        let data = vec![rt_byte, 0x03, 0x03, 0x00, 0x01, 0xAA];
        let frame = TlsEncryptedFrame::new(Bytes::from(data), rt);
        prop_assert_eq!(frame.record_type().as_u8(), rt_byte);
    }

    #[test]
    fn no_panic_on_arbitrary_bytes(data in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = parse_tls_frames(Cursor::new(data));
    }

    #[test]
    fn non_handshake_has_no_routing_key(
        rt_byte in prop_oneof![Just(20u8), Just(21u8), Just(23u8)],
        payload in prop::collection::vec(any::<u8>(), 1..128),
    ) {
        let rt = TlsRecordType::from_u8(rt_byte).unwrap();
        let frame = TlsEncryptedFrame::new(Bytes::from(payload), rt);
        prop_assert_eq!(frame.routing_key(), None);
    }

    #[test]
    fn sni_absent_when_no_sni_extension(
        sid_len in 0u8..32,
        cs_count in 1usize..16,
        ext_count in 1usize..5,
    ) {
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0x03, 0x03]);
        payload.extend_from_slice(&[0u8; 32]);
        payload.push(sid_len);
        payload.extend(std::iter::repeat(0u8).take(sid_len as usize));
        let cs_bytes_len = cs_count * 2;
        payload.push((cs_bytes_len >> 8) as u8);
        payload.push((cs_bytes_len & 0xff) as u8);
        for _ in 0..cs_count {
            payload.extend_from_slice(&[0x00, 0x9c]);
        }
        payload.push(0x01);
        payload.push(0x00);

        let mut extensions = Vec::new();
        for i in 0..ext_count {
            let ext_type = (0xff01u16 + i as u16).to_be_bytes();
            extensions.extend_from_slice(&ext_type);
            extensions.extend_from_slice(&[0x00, 0x02, 0xde, 0xad]);
        }
        payload.push((extensions.len() >> 8) as u8);
        payload.push((extensions.len() & 0xff) as u8);
        payload.extend_from_slice(&extensions);

        let body_len = payload.len();
        let mut handshake = Vec::new();
        handshake.push(0x01);
        handshake.push(((body_len >> 16) & 0xff) as u8);
        handshake.push(((body_len >> 8) & 0xff) as u8);
        handshake.push((body_len & 0xff) as u8);
        handshake.extend_from_slice(&payload);

        let frame = TlsEncryptedFrame::new(Bytes::from(handshake), TlsRecordType::Handshake);
        prop_assert_eq!(frame.routing_key(), None);
    }

    #[test]
    fn oversized_payload_rejected(extra in 1usize..1000) {
        let payload_len = 16384 + extra;
        let mut data = vec![23u8, 0x03, 0x03];
        data.push((payload_len >> 8) as u8);
        data.push((payload_len & 0xff) as u8);
        data.extend(std::iter::repeat(0xAA).take(payload_len));

        let rt = Runtime::new().unwrap();
        let result = rt.block_on(async {
            let cursor = Cursor::new(data);
            let mut parser = TlsParser::new();
            let stream = parser.parse(cursor);
            tokio::pin!(stream);
            stream.next().await
        });

        match result {
            Some(Err(_)) => {}
            other => prop_assert!(false, "expected error for oversized payload, got: {:?}", other.is_some()),
        }
    }
}
