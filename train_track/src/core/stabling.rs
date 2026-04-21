use std::collections::VecDeque;
use std::time::{Duration, Instant};
use bytes::Bytes;
use dashmap::DashMap;
use crate::io::destination::StreamDestination;

pub struct StablingConfig {
    pub max_idle_per_host: usize,
    pub max_total_idle: usize,
    pub idle_timeout: Duration,
    pub enabled: bool,
}

impl Default for StablingConfig {
    fn default() -> Self {
        Self {
            max_idle_per_host: 8,
            max_total_idle: 64,
            idle_timeout: Duration::from_secs(90),
            enabled: true,
        }
    }
}

struct StabledConnection<D> {
    dest: D,
    stabled_at: Instant,
}

pub struct Stabling<D: StreamDestination> {
    config: StablingConfig,
    idle: DashMap<Bytes, VecDeque<StabledConnection<D>>>,
}

impl<D: StreamDestination> Stabling<D> {
    pub fn new(config: StablingConfig) -> Self {
        Self {
            config,
            idle: DashMap::new(),
        }
    }

    pub fn acquire(&self, routing_key: &[u8]) -> Option<D> {
        if !self.config.enabled {
            return None;
        }
        let key = Bytes::copy_from_slice(routing_key);
        let mut entry = self.idle.get_mut(&key)?;
        let queue = entry.value_mut();
        while let Some(conn) = queue.pop_back() {
            if conn.stabled_at.elapsed() < self.config.idle_timeout {
                if queue.is_empty() {
                    drop(entry);
                    self.idle.remove(&key);
                }
                return Some(conn.dest);
            }
        }
        drop(entry);
        self.idle.remove(&key);
        None
    }

    pub fn release(&self, routing_key: Bytes, dest: D) {
        if !self.config.enabled {
            return;
        }
        let mut entry = self.idle.entry(routing_key).or_insert_with(VecDeque::new);
        let queue = entry.value_mut();

        if queue.len() >= self.config.max_idle_per_host {
            queue.pop_front();
        }

        queue.push_back(StabledConnection {
            dest,
            stabled_at: Instant::now(),
        });
    }

    pub fn reap_expired(&self) {
        let timeout = self.config.idle_timeout;
        self.idle.retain(|_, queue| {
            queue.retain(|conn| conn.stabled_at.elapsed() < timeout);
            !queue.is_empty()
        });
    }

    pub fn idle_count(&self) -> usize {
        self.idle.iter().map(|e| e.value().len()).sum()
    }
}
