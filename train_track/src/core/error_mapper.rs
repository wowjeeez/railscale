use bytes::Bytes;
use crate::atom::frame::Frame;
use crate::RailscaleError;

pub trait ErrorToFrames {
    type Frame: Frame;
    fn error_frames(&self, err: &RailscaleError) -> Vec<Self::Frame>;
}

pub trait ErrorToBytes {
    fn error_bytes(&self, err: &RailscaleError) -> Bytes;
}

impl<T: ErrorToFrames> ErrorToBytes for T {
    fn error_bytes(&self, err: &RailscaleError) -> Bytes {
        let frames = self.error_frames(err);
        let total_len: usize = frames.iter().map(|f| f.as_bytes().len()).sum();
        let mut buf = bytes::BytesMut::with_capacity(total_len);
        for frame in frames {
            buf.extend_from_slice(frame.as_bytes());
        }
        buf.freeze()
    }
}
