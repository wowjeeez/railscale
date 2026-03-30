use train_track::{ErrorToFrames, ErrorKind, RailscaleError};
use crate::HttpFrame;

pub struct HttpErrorResponder;

fn status_for_error(err: &RailscaleError) -> (u16, &'static str) {
    match &err.kind {
        ErrorKind::Parse(_) => (400, "Bad Request"),
        ErrorKind::RoutingFailed(_) => (502, "Bad Gateway"),
        ErrorKind::ConnectionClosed => (499, "Client Closed"),
        ErrorKind::NoRoutingFrame => (400, "Bad Request"),
        ErrorKind::BufferLimitExceeded => (413, "Payload Too Large"),
        ErrorKind::Io(_) => (502, "Bad Gateway"),
    }
}

impl ErrorToFrames for HttpErrorResponder {
    type Frame = HttpFrame;

    fn error_frames(&self, err: &RailscaleError) -> Vec<HttpFrame> {
        let (code, reason) = status_for_error(err);
        let body = format!("{code} {reason}\r\n");
        let body_len = body.len();

        let status_line = format!("HTTP/1.1 {code} {reason}\r\n");
        let headers = format!(
            "Content-Type: text/plain\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
        );

        let mut response = bytes::BytesMut::with_capacity(status_line.len() + headers.len() + body_len);
        response.extend_from_slice(status_line.as_bytes());
        response.extend_from_slice(headers.as_bytes());
        response.extend_from_slice(body.as_bytes());

        vec![HttpFrame::header(response.freeze(), false)]
    }
}
