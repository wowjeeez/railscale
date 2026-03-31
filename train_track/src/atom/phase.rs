use bytes::Bytes;
use super::frame::Frame;

pub trait FramePhase: Copy + Clone + Ord + Eq + Send + Sync + 'static {
    fn is_reorderable(&self) -> bool;
}

pub trait PhasedFrame: Frame {
    type Phase: FramePhase;
    fn phase(&self) -> Self::Phase;
}

enum Entry<F: PhasedFrame> {
    Framed { phase: F::Phase, data: Bytes },
    Passthrough { phase: F::Phase, data: Bytes },
}

impl<F: PhasedFrame> Entry<F> {
    fn phase(&self) -> F::Phase {
        match self {
            Entry::Framed { phase, .. } => *phase,
            Entry::Passthrough { phase, .. } => *phase,
        }
    }

    fn data(&self) -> &Bytes {
        match self {
            Entry::Framed { data, .. } => data,
            Entry::Passthrough { data, .. } => data,
        }
    }

    fn into_data(self) -> Bytes {
        match self {
            Entry::Framed { data, .. } => data,
            Entry::Passthrough { data, .. } => data,
        }
    }
}

pub struct PhasedBuffer<F: PhasedFrame> {
    entries: Vec<Entry<F>>,
    byte_count: usize,
}

impl<F: PhasedFrame> PhasedBuffer<F> {
    pub fn new() -> Self {
        Self { entries: Vec::new(), byte_count: 0 }
    }

    pub fn push_frame(&mut self, frame: F) {
        let phase = frame.phase();
        let data = frame.into_bytes();
        self.byte_count += data.len();
        self.entries.push(Entry::Framed { phase, data });
    }

    pub fn push_passthrough(&mut self, phase: F::Phase, data: Bytes) {
        self.byte_count += data.len();
        self.entries.push(Entry::Passthrough { phase, data });
    }

    pub fn reorder(&mut self) {
        self.entries.sort_by(|a, b| {
            let phase_cmp = a.phase().cmp(&b.phase());
            if phase_cmp == std::cmp::Ordering::Equal {
                std::cmp::Ordering::Equal
            } else {
                phase_cmp
            }
        });
    }

    pub fn byte_count(&self) -> usize {
        self.byte_count
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn drain(self) -> impl Iterator<Item = Bytes> {
        self.entries.into_iter().map(|e| e.into_data())
    }

    pub fn freeze(self) -> Bytes {
        let mut buf = bytes::BytesMut::with_capacity(self.byte_count);
        for entry in self.entries {
            buf.extend_from_slice(entry.data());
        }
        buf.freeze()
    }

    pub fn phases_present(&self) -> Vec<F::Phase> {
        let mut phases: Vec<F::Phase> = self.entries.iter().map(|e| e.phase()).collect();
        phases.dedup();
        phases
    }
}
