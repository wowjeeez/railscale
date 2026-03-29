use bytes::Bytes;
use train_track::{Frame, FramePipeline};

struct TestFrame(Bytes);

impl Frame for TestFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn is_routing_frame(&self) -> bool { false }
}

struct UppercasePipeline;

impl FramePipeline for UppercasePipeline {
    type Frame = TestFrame;

    fn process(&self, frame: Self::Frame) -> Self::Frame {
        let upper: Vec<u8> = frame.as_bytes().to_ascii_uppercase();
        TestFrame(Bytes::from(upper))
    }
}

#[test]
fn pipeline_transforms_frame() {
    let pipeline = UppercasePipeline;
    let frame = TestFrame(Bytes::from_static(b"host: example.com"));
    let result = pipeline.process(frame);
    assert_eq!(result.as_bytes(), b"HOST: EXAMPLE.COM");
}

#[test]
fn pipeline_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<UppercasePipeline>();
}
