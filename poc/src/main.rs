mod codec;
mod stream;

use std::net::SocketAddr;
use std::time::Instant;
use bytes::Bytes;
use memchr::memmem::Finder;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::StreamExt;
use crate::stream::{MappedFrame, MappedHttpStream};

async fn handle_stream(mut stream: TcpStream, addr: SocketAddr) {
    let start = Instant::now();
    let (read, mut write) = stream.split();

    let matchers = vec![
        (Finder::new(b"Host"), Bytes::from_static(b"rewritten.local")),
        (Finder::new(b"User-Agent"), Bytes::from_static(b"railscale-poc/0.1")),
    ];

    let mut mapped = MappedHttpStream::from(read, matchers);
    let mut header_count = 0;
    let mut body_bytes = 0;

    while let Some(Ok(frame)) = mapped.next().await {
        match frame {
            MappedFrame::Header(h) => {
                header_count += 1;
                println!("[{addr}] header: {}", String::from_utf8_lossy(&h));
            }
            MappedFrame::Body(b) => {
                body_bytes += b.len();
                println!("[{addr}] body chunk: {} bytes", b.len());
            }
        }
    }

    let elapsed = start.elapsed();
    println!("[{addr}] {header_count} headers + {body_bytes} body bytes in {}us", elapsed.as_micros());

    let body = format!("railscale poc — {header_count} headers parsed\n");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = write.write_all(response.as_bytes()).await;
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("railscale poc listening on {addr}");

    loop {
        let (stream, peer) = listener.accept().await.unwrap();
        tokio::spawn(handle_stream(stream, peer));
    }
}
