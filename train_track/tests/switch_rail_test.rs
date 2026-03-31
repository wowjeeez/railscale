use bytes::Bytes;
use train_track::{Frame, SwitchRail, IdentityRail};

struct TestFrame(Bytes);

impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct DoubleFrame(Bytes);

impl Frame for DoubleFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct TestToDoubleRail;

impl SwitchRail for TestToDoubleRail {
    type Input = TestFrame;
    type Output = DoubleFrame;

    fn switch(&self, input: TestFrame) -> DoubleFrame {
        let mut doubled = input.0.to_vec();
        doubled.extend_from_slice(&doubled.clone());
        DoubleFrame(Bytes::from(doubled))
    }
}

#[test]
fn switch_rail_converts_frame_type() {
    let rail = TestToDoubleRail;
    let input = TestFrame(Bytes::from_static(b"hello"));
    let output = rail.switch(input);
    assert_eq!(output.as_bytes(), b"hellohello");
}

#[test]
fn identity_rail_passes_through() {
    let rail = IdentityRail::<TestFrame>::new();
    let input = TestFrame(Bytes::from_static(b"unchanged"));
    let output = rail.switch(input);
    assert_eq!(output.as_bytes(), b"unchanged");
}

#[test]
fn switch_rail_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TestToDoubleRail>();
    assert_send_sync::<IdentityRail<TestFrame>>();
}
