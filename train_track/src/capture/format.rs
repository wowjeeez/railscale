use std::io::Write;

pub fn write_shb(w: &mut impl Write) -> std::io::Result<()> {
    let block_type: u32 = 0x0A0D0D0A;
    let total_length: u32 = 28;
    let byte_order_magic: u32 = 0x1A2B3C4D;
    let major_version: u16 = 1;
    let minor_version: u16 = 0;
    let section_length: i64 = -1;

    w.write_all(&block_type.to_le_bytes())?;
    w.write_all(&total_length.to_le_bytes())?;
    w.write_all(&byte_order_magic.to_le_bytes())?;
    w.write_all(&major_version.to_le_bytes())?;
    w.write_all(&minor_version.to_le_bytes())?;
    w.write_all(&section_length.to_le_bytes())?;
    w.write_all(&total_length.to_le_bytes())?;
    Ok(())
}

pub fn write_idb(w: &mut impl Write) -> std::io::Result<()> {
    let block_type: u32 = 0x00000001;
    let total_length: u32 = 20;
    let link_type: u16 = 147;
    let reserved: u16 = 0;
    let snap_len: u32 = 0;

    w.write_all(&block_type.to_le_bytes())?;
    w.write_all(&total_length.to_le_bytes())?;
    w.write_all(&link_type.to_le_bytes())?;
    w.write_all(&reserved.to_le_bytes())?;
    w.write_all(&snap_len.to_le_bytes())?;
    w.write_all(&total_length.to_le_bytes())?;
    Ok(())
}

pub fn write_epb(
    w: &mut impl Write,
    interface_id: u32,
    timestamp_us: u64,
    data: &[u8],
    direction: &str,
    connection_id: u64,
) -> std::io::Result<()> {
    let block_type: u32 = 0x00000006;
    let ts_high = (timestamp_us >> 32) as u32;
    let ts_low = (timestamp_us & 0xFFFF_FFFF) as u32;
    let captured_len = data.len() as u32;
    let original_len = captured_len;

    let pad = (4 - (data.len() % 4)) % 4;
    let padded_data_len = data.len() + pad;

    let comment = format!("{} conn={}", direction, connection_id);
    let comment_bytes = comment.as_bytes();
    let comment_pad = (4 - (comment_bytes.len() % 4)) % 4;
    let padded_comment_len = comment_bytes.len() + comment_pad;

    let options_len = 4 + padded_comment_len + 4;

    let total_length = (32 + padded_data_len + options_len) as u32;

    w.write_all(&block_type.to_le_bytes())?;
    w.write_all(&total_length.to_le_bytes())?;
    w.write_all(&interface_id.to_le_bytes())?;
    w.write_all(&ts_high.to_le_bytes())?;
    w.write_all(&ts_low.to_le_bytes())?;
    w.write_all(&captured_len.to_le_bytes())?;
    w.write_all(&original_len.to_le_bytes())?;

    w.write_all(data)?;
    if pad > 0 {
        w.write_all(&vec![0u8; pad])?;
    }

    let opt_comment_code: u16 = 1;
    let opt_comment_len: u16 = comment_bytes.len() as u16;
    w.write_all(&opt_comment_code.to_le_bytes())?;
    w.write_all(&opt_comment_len.to_le_bytes())?;
    w.write_all(comment_bytes)?;
    if comment_pad > 0 {
        w.write_all(&vec![0u8; comment_pad])?;
    }

    let opt_endofopt_code: u16 = 0;
    let opt_endofopt_len: u16 = 0;
    w.write_all(&opt_endofopt_code.to_le_bytes())?;
    w.write_all(&opt_endofopt_len.to_le_bytes())?;

    w.write_all(&total_length.to_le_bytes())?;
    Ok(())
}
