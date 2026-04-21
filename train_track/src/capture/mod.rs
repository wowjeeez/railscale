pub mod format;
pub mod pcap;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::capture::pcap::PcapDestination;

pub enum Direction {
    Request,
    Response,
}

struct CaptureEvent {
    direction: Direction,
    data: Bytes,
    connection_id: u64,
}

pub struct CaptureHandle {
    tx: mpsc::Sender<CaptureEvent>,
    connection_counter: Arc<AtomicU64>,
}

impl CaptureHandle {
    pub fn spawn(turnout_name: String, capture_dir: PathBuf) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<CaptureEvent>(4096);
        let counter = Arc::new(AtomicU64::new(0));

        let background = async move {
            let mut dest = PcapDestination::new(turnout_name, capture_dir);
            while let Some(event) = rx.recv().await {
                let dir_str = match event.direction {
                    Direction::Request => "req",
                    Direction::Response => "resp",
                };
                let _ = dest.write_event(&event.data, dir_str, event.connection_id);
            }
            dest.flush();
        };

        let task = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.spawn(background),
            Err(_) => {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                let task = rt.spawn(background);
                std::mem::forget(rt);
                task
            }
        };

        (Self { tx, connection_counter: counter }, task)
    }

    pub fn send(&self, direction: Direction, data: Bytes, connection_id: u64) {
        let _ = self.tx.try_send(CaptureEvent { direction, data, connection_id });
    }

    pub fn next_connection_id(&self) -> u64 {
        self.connection_counter.fetch_add(1, Ordering::Relaxed)
    }
}

impl Clone for CaptureHandle {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            connection_counter: Arc::clone(&self.connection_counter),
        }
    }
}
