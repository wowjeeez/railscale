use std::io;

use bytes::BytesMut;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::Decoder;

use crate::carriage::passengers::http::HttpPassenger;
use crate::carriage::ticket_pipeline::{PassengerDecoder, TicketField};

pub struct DevListener {
    listener: TcpListener,
}

impl DevListener {
    pub async fn bind(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        eprintln!("[dev] listening on {}", listener.local_addr()?);
        Ok(Self { listener })
    }

    pub async fn run(&self) -> io::Result<()> {
        loop {
            let (stream, peer) = self.listener.accept().await?;
            eprintln!("[dev] connection from {peer}");
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream).await {
                    eprintln!("[dev] error handling {peer}: {e}");
                }
            });
        }
    }
}

async fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut raw = BytesMut::with_capacity(4096);
    let mut decoder = HttpPassenger::with_predicate(|_| true);

    // Read until we have the full header section
    loop {
        let n = stream.read_buf(&mut raw).await?;
        if n == 0 {
            return Ok(());
        }

        // Drain all available fields from the decoder
        loop {
            match decoder.decode(&mut raw)? {
                Some(TicketField::Boundary) => {
                    // Headers done — respond
                    let body = "railscale dev server\n";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body,
                    );
                    tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await?;
                    return Ok(());
                }
                Some(field) => {
                    log_field(&field);
                }
                None => break, // need more data
            }
        }
    }
}

fn log_field(field: &TicketField) {
    match field {
        TicketField::Buffered(bf) => {
            use crate::carriage::ticket_pipeline::BufferedField;
            match bf {
                BufferedField::Attribute(a) => eprintln!("[dev]   > {a}"),
                BufferedField::Header(k, v) => eprintln!("[dev]   {k}: {v}"),
                BufferedField::KeyValue(k, v) => eprintln!("[dev]   {k}={v}"),
                BufferedField::Bytes(b) => eprintln!("[dev]   ({} raw bytes)", b.len()),
            }
        }
        TicketField::Passthrough(b) => {
            eprintln!("[dev]   passthrough ({} bytes)", b.len());
        }
        TicketField::Boundary => {}
    }
}
