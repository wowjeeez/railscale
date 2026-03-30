use train_track::{RailscaleError, ErrorKind, Phase};

#[test]
fn io_error_converts() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
    let rail_err: RailscaleError = io_err.into();
    assert!(matches!(rail_err.kind, ErrorKind::Io(_)));
}

#[test]
fn displays_variants() {
    let err = RailscaleError::from(ErrorKind::Parse("bad header".into()));
    let msg = format!("{err}");
    assert!(msg.contains("bad header"));
}

#[test]
fn phase_display() {
    let err = RailscaleError::from(ErrorKind::Io(
        std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused"),
    ))
    .in_phase(Phase::Routing);
    let msg = format!("{err}");
    assert!(msg.contains("[routing]"));
    assert!(msg.contains("connection refused"));
}

#[test]
fn buffer_limit_display() {
    let err = RailscaleError::from(ErrorKind::BufferLimitExceeded);
    let msg = format!("{err}");
    assert!(msg.contains("buffer limit exceeded"));
}
