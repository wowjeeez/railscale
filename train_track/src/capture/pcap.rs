use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

use bytes::Bytes;

use crate::StreamDestination;
use crate::capture::format::{write_epb, write_idb, write_shb};

pub struct PcapDestination {
    writer: Option<BufWriter<File>>,
    turnout_name: String,
    capture_dir: PathBuf,
    request_count: u32,
    next_is_request: bool,
    file_start: Instant,
    file_index: u32,
    empty: tokio::io::Empty,
}

impl PcapDestination {
    pub fn new(turnout_name: String, capture_dir: PathBuf) -> Self {
        Self {
            writer: None,
            turnout_name,
            capture_dir,
            request_count: 0,
            next_is_request: true,
            file_start: Instant::now(),
            file_index: 0,
            empty: tokio::io::empty(),
        }
    }

    fn open_file(&mut self) -> std::io::Result<()> {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let filename = format!("railscale-{}-{}-{}.pcapng", self.turnout_name, timestamp, self.file_index);
        self.file_index += 1;
        let path = self.capture_dir.join(filename);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        let mut w = BufWriter::new(file);
        write_shb(&mut w)?;
        write_idb(&mut w)?;
        self.writer = Some(w);
        self.file_start = Instant::now();
        Ok(())
    }

    pub fn flush(&mut self) {
        if let Some(ref mut w) = self.writer {
            let _ = w.flush();
        }
    }

    pub fn write_event(&mut self, data: &[u8], direction: &str, connection_id: u64) -> std::io::Result<()> {
        if self.writer.is_none() {
            self.open_file()?;
        }

        let elapsed_us = self.file_start.elapsed().as_micros() as u64;

        if let Some(ref mut w) = self.writer {
            write_epb(w, 0, elapsed_us, data, direction, connection_id)?;
        }

        if direction == "req" {
            self.request_count += 1;
        }

        if let Some(ref mut w) = self.writer {
            w.flush()?;
        }

        if direction == "resp" && self.request_count >= 10 {
            self.writer = None;
            self.request_count = 0;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl StreamDestination for PcapDestination {
    type Error = std::io::Error;
    type ResponseReader = tokio::io::Empty;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        let direction = if self.next_is_request { "req" } else { "resp" };
        self.next_is_request = !self.next_is_request;
        self.write_event(&bytes, direction, 0)
    }

    fn response_reader(&mut self) -> &mut tokio::io::Empty {
        &mut self.empty
    }
}
