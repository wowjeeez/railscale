use bytes::Bytes;
use train_track::Frame;

pub struct HttpFrame {
    data: Bytes,
    routing: bool,
    end_of_headers: bool,
}

impl HttpFrame {
    pub fn header(data: Bytes, is_request_line: bool) -> Self {
        Self { data, routing: is_request_line, end_of_headers: false }
    }

    pub fn end_of_headers() -> Self {
        Self { data: Bytes::new(), routing: false, end_of_headers: true }
    }

    pub fn is_end_of_headers(&self) -> bool {
        self.end_of_headers
    }
}

impl Frame for HttpFrame {
    fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    fn into_bytes(self) -> Bytes {
        self.data
    }

    fn is_routing_frame(&self) -> bool {
        self.routing
    }
}
