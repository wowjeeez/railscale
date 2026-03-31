use bytes::Bytes;
use train_track::{Frame, FramePhase, PhasedFrame};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum HttpPhase {
    RequestLine,
    Header,
    EndOfHeaders,
    Body,
    Trailer,
}

impl FramePhase for HttpPhase {
    fn is_reorderable(&self) -> bool {
        matches!(self, HttpPhase::Header | HttpPhase::Trailer)
    }
}

pub struct HttpFrame {
    data: Bytes,
    phase: HttpPhase,
    routing: bool,
}

impl HttpFrame {
    pub fn request_line(data: Bytes) -> Self {
        Self { data, phase: HttpPhase::RequestLine, routing: true }
    }

    pub fn header(data: Bytes) -> Self {
        Self { data, phase: HttpPhase::Header, routing: false }
    }

    pub fn end_of_headers() -> Self {
        Self { data: Bytes::new(), phase: HttpPhase::EndOfHeaders, routing: false }
    }

    pub fn body(data: Bytes) -> Self {
        Self { data, phase: HttpPhase::Body, routing: false }
    }

    pub fn trailer(data: Bytes) -> Self {
        Self { data, phase: HttpPhase::Trailer, routing: false }
    }

    pub fn is_end_of_headers(&self) -> bool {
        self.phase == HttpPhase::EndOfHeaders
    }
}

impl Frame for HttpFrame {
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

impl PhasedFrame for HttpFrame {
    type Phase = HttpPhase;

    fn phase(&self) -> HttpPhase {
        self.phase
    }
}
