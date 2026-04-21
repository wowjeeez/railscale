use bytes::Bytes;
use std::time::Duration;
use train_track::{Stabling, StablingConfig, StreamDestination};

struct MockDest {
    id: u32,
    empty: tokio::io::Empty,
}

impl MockDest {
    fn new(id: u32) -> Self {
        Self { id, empty: tokio::io::empty() }
    }
}

#[async_trait::async_trait]
impl StreamDestination for MockDest {
    type Error = std::io::Error;
    type ResponseReader = tokio::io::Empty;

    async fn write(&mut self, _bytes: Bytes) -> Result<(), Self::Error> {
        Ok(())
    }

    fn response_reader(&mut self) -> &mut tokio::io::Empty {
        &mut self.empty
    }
}

#[test]
fn acquire_from_empty_returns_none() {
    let stabling = Stabling::<MockDest>::new(StablingConfig::default());
    assert!(stabling.acquire(b"127.0.0.1:8080").is_none());
}

#[test]
fn release_then_acquire() {
    let stabling = Stabling::<MockDest>::new(StablingConfig::default());
    let key = Bytes::from_static(b"127.0.0.1:8080");
    stabling.release(key.clone(), MockDest::new(1));
    let dest = stabling.acquire(b"127.0.0.1:8080").unwrap();
    assert_eq!(dest.id, 1);
    assert!(stabling.acquire(b"127.0.0.1:8080").is_none());
}

#[test]
fn lifo_ordering() {
    let stabling = Stabling::<MockDest>::new(StablingConfig::default());
    let key = Bytes::from_static(b"host");
    stabling.release(key.clone(), MockDest::new(1));
    stabling.release(key.clone(), MockDest::new(2));
    assert_eq!(stabling.acquire(b"host").unwrap().id, 2);
    assert_eq!(stabling.acquire(b"host").unwrap().id, 1);
}

#[test]
fn per_host_limit_evicts_oldest() {
    let config = StablingConfig {
        max_idle_per_host: 2,
        ..StablingConfig::default()
    };
    let stabling = Stabling::<MockDest>::new(config);
    let key = Bytes::from_static(b"host");
    stabling.release(key.clone(), MockDest::new(1));
    stabling.release(key.clone(), MockDest::new(2));
    stabling.release(key.clone(), MockDest::new(3));
    assert_eq!(stabling.idle_count(), 2);
    assert_eq!(stabling.acquire(b"host").unwrap().id, 3);
    assert_eq!(stabling.acquire(b"host").unwrap().id, 2);
}

#[test]
fn disabled_stabling_never_acquires() {
    let config = StablingConfig {
        enabled: false,
        ..StablingConfig::default()
    };
    let stabling = Stabling::<MockDest>::new(config);
    stabling.release(Bytes::from_static(b"host"), MockDest::new(1));
    assert!(stabling.acquire(b"host").is_none());
}

#[test]
fn reap_expired_removes_old_connections() {
    let config = StablingConfig {
        idle_timeout: Duration::from_millis(1),
        ..StablingConfig::default()
    };
    let stabling = Stabling::<MockDest>::new(config);
    stabling.release(Bytes::from_static(b"host"), MockDest::new(1));
    std::thread::sleep(Duration::from_millis(10));
    stabling.reap_expired();
    assert_eq!(stabling.idle_count(), 0);
}

#[test]
fn different_keys_independent() {
    let stabling = Stabling::<MockDest>::new(StablingConfig::default());
    stabling.release(Bytes::from_static(b"a"), MockDest::new(1));
    stabling.release(Bytes::from_static(b"b"), MockDest::new(2));
    assert_eq!(stabling.acquire(b"a").unwrap().id, 1);
    assert_eq!(stabling.acquire(b"b").unwrap().id, 2);
}
