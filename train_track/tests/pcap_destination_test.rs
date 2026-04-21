#[cfg(feature = "capture")]
mod tests {
    use bytes::Bytes;
    use train_track::capture::pcap::PcapDestination;
    use train_track::StreamDestination;

    #[tokio::test]
    async fn creates_pcapng_file_on_first_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut dest = PcapDestination::new("test-proxy".into(), dir.path().to_path_buf());
        dest.write(Bytes::from_static(b"GET / HTTP/1.1\r\n\r\n")).await.unwrap();

        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().into_string().unwrap();
        assert!(name.starts_with("railscale-test-proxy-"));
        assert!(name.ends_with(".pcapng"));
    }

    #[tokio::test]
    async fn rotates_file_after_10_requests() {
        let dir = tempfile::tempdir().unwrap();
        let mut dest = PcapDestination::new("rotate".into(), dir.path().to_path_buf());

        for _ in 0..20 {
            dest.write(Bytes::from_static(b"request")).await.unwrap();
            dest.write(Bytes::from_static(b"response")).await.unwrap();
        }
        dest.flush();

        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn written_file_starts_with_shb_idb() {
        let dir = tempfile::tempdir().unwrap();
        let mut dest = PcapDestination::new("header".into(), dir.path().to_path_buf());
        dest.write(Bytes::from_static(b"data")).await.unwrap();
        dest.flush();

        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap()
            .filter_map(|e| e.ok())
            .collect();
        let content = std::fs::read(files[0].path()).unwrap();
        assert_eq!(&content[0..4], &0x0A0D0D0Au32.to_le_bytes());
        assert_eq!(&content[28..32], &0x00000001u32.to_le_bytes());
    }
}
