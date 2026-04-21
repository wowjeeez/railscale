#[cfg(feature = "capture")]
mod tests {
    use bytes::Bytes;
    use train_track::capture::{CaptureHandle, Direction};

    #[tokio::test]
    async fn end_to_end_capture_produces_valid_pcapng() {
        let dir = tempfile::tempdir().unwrap();
        let (handle, task) = CaptureHandle::spawn("e2e".into(), dir.path().to_path_buf());

        for i in 0..5 {
            handle.send(
                Direction::Request,
                Bytes::from(format!("GET /{} HTTP/1.1\r\nHost: test\r\n\r\n", i)),
                i,
            );
            handle.send(
                Direction::Response,
                Bytes::from(format!("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")),
                i,
            );
        }

        drop(handle);
        task.await.unwrap();

        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);

        let content = std::fs::read(files[0].path()).unwrap();
        assert_eq!(&content[0..4], &0x0A0D0D0Au32.to_le_bytes());
        assert_eq!(&content[28..32], &0x00000001u32.to_le_bytes());
        assert_eq!(&content[48..52], &0x00000006u32.to_le_bytes());
    }

    #[tokio::test]
    async fn capture_rotation_at_10_requests() {
        let dir = tempfile::tempdir().unwrap();
        let (handle, task) = CaptureHandle::spawn("rot".into(), dir.path().to_path_buf());

        for i in 0..15u64 {
            handle.send(
                Direction::Request,
                Bytes::from_static(b"GET / HTTP/1.1\r\n\r\n"),
                i,
            );
            handle.send(
                Direction::Response,
                Bytes::from_static(b"HTTP/1.1 200 OK\r\n\r\n"),
                i,
            );
        }

        drop(handle);
        task.await.unwrap();

        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 2);
    }
}
