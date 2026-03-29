use bytes::Bytes;
use memchr::memmem::Finder;
use train_track::{Frame, FramePipeline};
use locomotive::{HttpFrame, HttpPipeline};

#[test]
fn first_match_wins() {
    let pipeline = HttpPipeline::new(vec![
        (Finder::new(b"Host"), Bytes::from_static(b"first.com")),
        (Finder::new(b"Host"), Bytes::from_static(b"second.com")),
    ]);
    let frame = HttpFrame::header(Bytes::from_static(b"Host: original.com"), false);
    let result = pipeline.process(frame);
    assert_eq!(result.as_bytes(), b"Host: first.com");
}

#[test]
fn no_match_passes_through() {
    let pipeline = HttpPipeline::new(vec![
        (Finder::new(b"Host"), Bytes::from_static(b"replaced.com")),
    ]);
    let frame = HttpFrame::header(Bytes::from_static(b"Content-Type: text/html"), false);
    let result = pipeline.process(frame);
    assert_eq!(result.as_bytes(), b"Content-Type: text/html");
}

#[test]
fn empty_matchers_passes_through() {
    let pipeline = HttpPipeline::new(vec![]);
    let frame = HttpFrame::header(Bytes::from_static(b"Host: example.com"), false);
    let result = pipeline.process(frame);
    assert_eq!(result.as_bytes(), b"Host: example.com");
}

#[test]
fn pipeline_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<HttpPipeline>();
}
