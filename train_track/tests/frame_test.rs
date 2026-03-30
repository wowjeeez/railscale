use bytes::Bytes;
use train_track::{Frame, ParsedData};

struct TestFrame {
    data: Bytes,
    routing: bool,
}

impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    fn into_bytes(self) -> Bytes {
        self.data
    }

    fn routing_key(&self) -> Option<&[u8]> {
        if self.routing { Some(&self.data) } else { None }
    }
}

#[test]
fn frame_as_bytes() {
    let f = TestFrame { data: Bytes::from_static(b"GET / HTTP/1.1"), routing: true };
    assert_eq!(f.as_bytes(), b"GET / HTTP/1.1");
    assert!(f.routing_key().is_some());
}

#[test]
fn frame_into_bytes() {
    let f = TestFrame { data: Bytes::from_static(b"hello"), routing: false };
    assert!(f.routing_key().is_none());
    let b = f.into_bytes();
    assert_eq!(&b[..], b"hello");
}

#[test]
fn parsed_data_variants() {
    let parsed: ParsedData<TestFrame> = ParsedData::Parsed(TestFrame {
        data: Bytes::from_static(b"header"),
        routing: false,
    });
    assert!(matches!(parsed, ParsedData::Parsed(_)));

    let passthrough: ParsedData<TestFrame> = ParsedData::Passthrough(Bytes::from_static(b"body"));
    assert!(matches!(passthrough, ParsedData::Passthrough(_)));
}
