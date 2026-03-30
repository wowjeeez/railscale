use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub struct RequestEntry {
    pub t: f64,
    pub total_us: u64,
    pub route_us: u64,
    pub forward_us: u64,
    pub relay_us: u64,
    pub req_bytes: u64,
    pub resp_bytes: u64,
    pub error: bool,
}

pub struct RecorderHandle {
    active: Arc<AtomicI64>,
    upstreams: Arc<AtomicI64>,
    request_tx: mpsc::UnboundedSender<RequestEntry>,
    _task: JoinHandle<()>,
}

impl RecorderHandle {
    pub fn log_request(&self, entry: RequestEntry) {
        let _ = self.request_tx.send(entry);
    }

    pub fn conn_start(&self) {
        self.active.fetch_add(1, Ordering::Relaxed);
    }

    pub fn conn_end(&self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn upstream_open(&self) {
        self.upstreams.fetch_add(1, Ordering::Relaxed);
    }

    pub fn upstream_close(&self) {
        self.upstreams.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn start_recorder(path: &str) -> RecorderHandle {
    let active = Arc::new(AtomicI64::new(0));
    let upstreams = Arc::new(AtomicI64::new(0));
    let (request_tx, request_rx) = mpsc::unbounded_channel();

    let task_active = Arc::clone(&active);
    let task_upstreams = Arc::clone(&upstreams);
    let task_path = path.to_string();

    let task = tokio::spawn(async move {
        recorder_task(&task_path, request_rx, task_active, task_upstreams).await;
    });

    RecorderHandle {
        active,
        upstreams,
        request_tx,
        _task: task,
    }
}

async fn recorder_task(
    path: &str,
    mut rx: mpsc::UnboundedReceiver<RequestEntry>,
    active: Arc<AtomicI64>,
    upstreams: Arc<AtomicI64>,
) {
    let file = std::fs::File::create(path).expect("failed to create recorder log");
    let mut writer = BufWriter::with_capacity(64 * 1024, file);

    let pid = Pid::from_u32(std::process::id());
    let refresh = ProcessRefreshKind::nothing()
        .with_memory()
        .with_cpu();

    let mut sys = sysinfo::System::new();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[pid]),
        true,
        refresh,
    );
    tokio::time::sleep(Duration::from_millis(200)).await;

    let start = std::time::Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            entry = rx.recv() => {
                match entry {
                    Some(e) => {
                        let _ = writeln!(
                            writer,
                            r#"{{"type":"req","t":{:.4},"total_us":{},"route_us":{},"forward_us":{},"relay_us":{},"req_bytes":{},"resp_bytes":{},"error":{}}}"#,
                            e.t, e.total_us, e.route_us, e.forward_us, e.relay_us,
                            e.req_bytes, e.resp_bytes, e.error,
                        );
                    }
                    None => {
                        let _ = writer.flush();
                        return;
                    }
                }
            }
            _ = ticker.tick() => {
                sys.refresh_processes_specifics(
                    sysinfo::ProcessesToUpdate::Some(&[pid]),
                    true,
                    refresh,
                );
                let (rss, cpu) = sys
                    .process(pid)
                    .map(|p| (p.memory(), p.cpu_usage()))
                    .unwrap_or((0, 0.0));
                let t = start.elapsed().as_secs_f64();
                let active_val = active.load(Ordering::Relaxed);
                let upstreams_val = upstreams.load(Ordering::Relaxed);
                let _ = writeln!(
                    writer,
                    r#"{{"type":"sys","t":{t:.4},"rss":{rss},"cpu":{cpu:.2},"active":{active_val},"upstreams":{upstreams_val}}}"#,
                );
                let _ = writer.flush();
            }
        }
    }
}
