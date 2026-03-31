use bytes::Bytes;
use train_track::{Frame, FramePipeline, SwitchRail, Turnout, SimpleTurnout, IdentityRail, FramePipelineAdapter};

struct TestFrame(Bytes);

impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct UpperFrame(Bytes);

impl Frame for UpperFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct ToUpperRail;

impl SwitchRail for ToUpperRail {
    type Input = TestFrame;
    type Output = UpperFrame;
    fn switch(&self, input: TestFrame) -> UpperFrame {
        UpperFrame(Bytes::from(input.0.to_ascii_uppercase()))
    }
}

#[test]
fn simple_turnout_switches_and_handles() {
    let turnout = SimpleTurnout::new(
        ToUpperRail,
        |frame: UpperFrame| Some(frame),
    );
    let input = TestFrame(Bytes::from_static(b"hello"));
    let output = turnout.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"HELLO");
}

#[test]
fn simple_turnout_handler_can_drop() {
    let turnout = SimpleTurnout::new(
        ToUpperRail,
        |_frame: UpperFrame| None,
    );
    let input = TestFrame(Bytes::from_static(b"hello"));
    assert!(turnout.process(input).is_none());
}

#[test]
fn simple_turnout_with_identity_rail() {
    let turnout = SimpleTurnout::new(
        IdentityRail::<TestFrame>::new(),
        |frame: TestFrame| Some(frame),
    );
    let input = TestFrame(Bytes::from_static(b"pass"));
    let output = turnout.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"pass");
}

struct UppercasePipeline;

impl FramePipeline for UppercasePipeline {
    type Frame = TestFrame;
    fn process(&self, frame: Self::Frame) -> Self::Frame {
        TestFrame(Bytes::from(frame.0.to_ascii_uppercase()))
    }
}

#[test]
fn frame_pipeline_adapter_wraps_existing_pipeline() {
    let adapter = FramePipelineAdapter::new(UppercasePipeline);
    let input = TestFrame(Bytes::from_static(b"hello"));
    let output = adapter.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"HELLO");
}

#[test]
fn frame_pipeline_adapter_never_drops() {
    let adapter = FramePipelineAdapter::new(UppercasePipeline);
    let input = TestFrame(Bytes::from_static(b"test"));
    assert!(adapter.process(input).is_some());
}

#[test]
fn turnout_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SimpleTurnout<ToUpperRail, fn(UpperFrame) -> Option<UpperFrame>>>();
    assert_send_sync::<FramePipelineAdapter<UppercasePipeline>>();
}
