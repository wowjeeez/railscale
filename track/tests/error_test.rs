use train_track::RailscaleError;

#[test]
fn io_error_converts() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
    let rail_err: RailscaleError = io_err.into();
    assert!(matches!(rail_err, RailscaleError::Io(_)));
}

#[test]
fn displays_variants() {
    let err = RailscaleError::Parse("bad header".into());
    let msg = format!("{err}");
    assert!(msg.contains("bad header"));
}
