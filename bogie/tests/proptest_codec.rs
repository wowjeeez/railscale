use bogie::generators::{arb_header_line, arb_http_message, arb_request_line};
use bytes::BytesMut;
use carriage::http_v1::HttpStreamingCodec;
use proptest::prelude::*;
use tokio_util::codec::Decoder;
use train_track::Frame;

fn decode_all(codec: &mut HttpStreamingCodec, data: &[u8]) -> Vec<Vec<u8>> {
    let mut buf = BytesMut::from(data);
    let mut frames = Vec::new();
    loop {
        match codec.decode(&mut buf) {
            Ok(Some(frame)) => {
                if frame.is_end_of_headers() {
                    frames.push(b"\r\n".to_vec());
                    break;
                }
                frames.push(frame.into_bytes().to_vec());
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    frames
}

proptest! {
    #[test]
    fn split_invariance(msg in arb_http_message(), split_point in 0usize..1024) {
        let one_shot = {
            let mut codec = HttpStreamingCodec::new(vec![]);
            decode_all(&mut codec, &msg)
        };

        let split_at = split_point % msg.len().max(1);
        let (first, second) = msg.split_at(split_at);

        let mut codec = HttpStreamingCodec::new(vec![]);
        let mut buf = BytesMut::from(first);
        let mut split_frames = Vec::new();

        loop {
            match codec.decode(&mut buf) {
                Ok(Some(frame)) => {
                    if frame.is_end_of_headers() {
                        split_frames.push(b"\r\n".to_vec());
                        break;
                    }
                    split_frames.push(frame.into_bytes().to_vec());
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        if !codec.headers_done() {
            buf.extend_from_slice(second);
            loop {
                match codec.decode(&mut buf) {
                    Ok(Some(frame)) => {
                        if frame.is_end_of_headers() {
                            split_frames.push(b"\r\n".to_vec());
                            break;
                        }
                        split_frames.push(frame.into_bytes().to_vec());
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }

        prop_assert_eq!(one_shot, split_frames);
    }

    #[test]
    fn frame_count_matches_header_count(
        request_line in arb_request_line(),
        headers in prop::collection::vec(arb_header_line(), 0..10),
    ) {
        let mut msg = request_line;
        for h in &headers {
            msg.extend_from_slice(h);
        }
        msg.extend_from_slice(b"\r\n");

        let mut codec = HttpStreamingCodec::new(vec![]);
        let frames = decode_all(&mut codec, &msg);

        let expected = 1 + headers.len() + 1;
        prop_assert_eq!(frames.len(), expected);
    }

    #[test]
    fn round_trip_fidelity(msg in arb_http_message()) {
        let mut codec = HttpStreamingCodec::new(vec![]);
        let frames = decode_all(&mut codec, &msg);
        let reassembled: Vec<u8> = frames.into_iter().flatten().collect();
        prop_assert_eq!(msg, reassembled);
    }

    #[test]
    fn no_panic_on_arbitrary_bytes(data in prop::collection::vec(any::<u8>(), 0..512)) {
        let mut codec = HttpStreamingCodec::new(vec![]);
        let mut buf = BytesMut::from(data.as_slice());
        for _ in 0..100 {
            match codec.decode(&mut buf) {
                Ok(Some(frame)) => {
                    if frame.is_end_of_headers() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
            if codec.headers_done() {
                break;
            }
        }
    }
}
