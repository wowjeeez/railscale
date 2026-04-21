use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::fs::File;
use bytes::Bytes;
use crate::StreamDestination;

pub struct FileDestination {
    writer: BufWriter<File>,
    empty: tokio::io::Empty,
}

impl FileDestination {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        Ok(Self { writer: BufWriter::new(file), empty: tokio::io::empty() })
    }
}

#[async_trait::async_trait]
impl StreamDestination for FileDestination {
    type Error = std::io::Error;
    type ResponseReader = tokio::io::Empty;

    async fn write(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        self.writer.write_all(&bytes)
    }

    fn response_reader(&mut self) -> &mut tokio::io::Empty {
        &mut self.empty
    }
}
