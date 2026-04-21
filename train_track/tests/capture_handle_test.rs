#[cfg(feature = "capture")]
mod tests {
    use bytes::Bytes;
    use train_track::capture::{CaptureHandle, Direction};

    #[tokio::test]
    async fn capture_handle_sends_events() {
        let dir = tempfile::tempdir().unwrap();
        let (handle, task) = CaptureHandle::spawn("test".into(), dir.path().to_path_buf());

        handle.send(Direction::Request, Bytes::from_static(b"GET /\r\n\r\n"), 1);
        handle.send(Direction::Response, Bytes::from_static(b"HTTP/1.1 200 OK\r\n\r\n"), 1);

        drop(handle);
        task.await.unwrap();

        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);
    }

    #[tokio::test]
    async fn capture_handle_does_not_block_on_full_channel() {
        let dir = tempfile::tempdir().unwrap();
        let (handle, _task) = CaptureHandle::spawn("backpressure".into(), dir.path().to_path_buf());

        for i in 0..10000 {
            handle.send(Direction::Request, Bytes::from(format!("req {}", i)), i as u64);
        }
    }

    #[test]
    fn connection_id_increments() {
        let dir = tempfile::tempdir().unwrap();
        let (handle, _task) = CaptureHandle::spawn("ids".into(), dir.path().to_path_buf());
        assert_eq!(handle.next_connection_id(), 0);
        assert_eq!(handle.next_connection_id(), 1);
        assert_eq!(handle.next_connection_id(), 2);
    }
}
