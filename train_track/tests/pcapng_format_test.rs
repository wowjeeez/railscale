#[cfg(feature = "capture")]
use train_track::capture::format::{write_shb, write_idb, write_epb};

#[cfg(feature = "capture")]
#[test]
fn shb_has_correct_magic_and_length() {
    let mut buf = Vec::new();
    write_shb(&mut buf).unwrap();
    assert_eq!(&buf[0..4], &0x0A0D0D0Au32.to_le_bytes());
    assert_eq!(&buf[8..12], &0x1A2B3C4Du32.to_le_bytes());
    let total_len = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    assert_eq!(buf.len(), total_len as usize);
    assert_eq!(&buf[buf.len()-4..], &total_len.to_le_bytes());
}

#[cfg(feature = "capture")]
#[test]
fn idb_has_user0_link_type() {
    let mut buf = Vec::new();
    write_idb(&mut buf).unwrap();
    assert_eq!(&buf[0..4], &0x00000001u32.to_le_bytes());
    let link_type = u16::from_le_bytes(buf[8..10].try_into().unwrap());
    assert_eq!(link_type, 147);
}

#[cfg(feature = "capture")]
#[test]
fn epb_wraps_payload_with_padding() {
    let mut buf = Vec::new();
    let payload = b"GET / HTTP/1.1\r\n\r\n";
    write_epb(&mut buf, 0, 1000, payload, "req", 42).unwrap();
    assert_eq!(&buf[0..4], &0x00000006u32.to_le_bytes());
    let total_len = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    assert_eq!(buf.len(), total_len as usize);
    let captured_len = u32::from_le_bytes(buf[20..24].try_into().unwrap());
    assert_eq!(captured_len, payload.len() as u32);
}
