use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, System, CpuRefreshKind};
use tokio::sync::mpsc;

pub struct SamplerHandle {
    shared: Arc<SharedCounters>,
    request_tx: mpsc::UnboundedSender<RequestRecord>,
    _sampler_guard: Option<tokio::task::JoinHandle<()>>,
    _writer_guard: Option<tokio::task::JoinHandle<()>>,
}

pub struct SharedCounters {
    pub active_connections: AtomicI64,
}

impl SharedCounters {
    pub fn new() -> Self {
        Self {
            active_connections: AtomicI64::new(0),
        }
    }
}

pub struct RequestRecord {
    pub t: f64,
    pub total_us: u64,
    pub connect_us: u64,
    pub forward_us: u64,
    pub relay_us: u64,
    pub frames: u64,
    pub req_bytes: u64,
    pub resp_bytes: u64,
    pub error: bool,
}

impl SamplerHandle {
    pub fn shared(&self) -> &Arc<SharedCounters> {
        &self.shared
    }

    pub fn log_request(&self, record: RequestRecord) {
        let _ = self.request_tx.send(record);
    }
}

pub fn start_sampler(path: &str, interval: Duration) -> SamplerHandle {
    let shared = Arc::new(SharedCounters::new());
    let shared_clone = Arc::clone(&shared);
    let system_path = path.to_string();

    let request_path = system_path.replace(".jsonl", "-requests.jsonl");
    let (request_tx, request_rx) = mpsc::unbounded_channel();

    let sampler_handle = tokio::spawn(async move {
        sampler_loop(&system_path, interval, &shared_clone).await;
    });

    let writer_handle = tokio::spawn(async move {
        request_writer_loop(&request_path, request_rx).await;
    });

    SamplerHandle {
        shared,
        request_tx,
        _sampler_guard: Some(sampler_handle),
        _writer_guard: Some(writer_handle),
    }
}

async fn request_writer_loop(path: &str, mut rx: mpsc::UnboundedReceiver<RequestRecord>) {
    let file = std::fs::File::create(path).expect("failed to create request log");
    let mut writer = BufWriter::with_capacity(64 * 1024, file);

    let mut count = 0u64;
    while let Some(r) = rx.recv().await {
        let _ = writeln!(
            writer,
            r#"{{"t":{:.4},"total_us":{},"connect_us":{},"forward_us":{},"relay_us":{},"frames":{},"req_bytes":{},"resp_bytes":{},"error":{}}}"#,
            r.t, r.total_us, r.connect_us, r.forward_us, r.relay_us,
            r.frames, r.req_bytes, r.resp_bytes, r.error,
        );
        count += 1;
        if count % 100 == 0 {
            let _ = writer.flush();
        }
    }
    let _ = writer.flush();
}

async fn sampler_loop(path: &str, interval: Duration, counters: &SharedCounters) {
    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new();
    let refresh = ProcessRefreshKind::nothing()
        .with_memory()
        .with_cpu();

    let mut file = std::fs::File::create(path).expect("failed to create metrics file");

    let mut cap_sys = System::new();
    cap_sys.refresh_memory();
    cap_sys.refresh_cpu_specifics(CpuRefreshKind::nothing());
    let total_mem = cap_sys.total_memory();
    let cpu_count = cap_sys.cpus().len();
    let _ = writeln!(
        file,
        r#"{{"type":"capacity","total_mem":{total_mem},"cpu_count":{cpu_count}}}"#,
    );
    let _ = file.flush();

    let start = Instant::now();

    sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid]), true, refresh);
    tokio::time::sleep(Duration::from_millis(200)).await;

    loop {
        sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid]), true, refresh);

        let (rss_bytes, cpu_pct) = sys
            .process(pid)
            .map(|p| (p.memory(), p.cpu_usage()))
            .unwrap_or((0, 0.0));

        let elapsed = start.elapsed().as_secs_f64();
        let active = counters.active_connections.load(Ordering::Relaxed);

        let _ = writeln!(
            file,
            r#"{{"t":{elapsed:.4},"rss":{rss_bytes},"cpu":{cpu_pct:.2},"active":{active}}}"#,
        );
        let _ = file.flush();

        tokio::time::sleep(interval).await;
    }
}
