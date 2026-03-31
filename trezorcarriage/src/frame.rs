use bytes::Bytes;
use train_track::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsRecordType {
    ChangeCipherSpec,
    Alert,
    Handshake,
    ApplicationData,
}

impl TlsRecordType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            20 => Some(Self::ChangeCipherSpec),
            21 => Some(Self::Alert),
            22 => Some(Self::Handshake),
            23 => Some(Self::ApplicationData),
            _ => None,
        }
    }

    pub fn as_u8(&self) -> u8 {
        match self {
            Self::ChangeCipherSpec => 20,
            Self::Alert => 21,
            Self::Handshake => 22,
            Self::ApplicationData => 23,
        }
    }
}

pub struct TlsEncryptedFrame {
    data: Bytes,
    record_type: TlsRecordType,
}

impl TlsEncryptedFrame {
    pub fn new(data: Bytes, record_type: TlsRecordType) -> Self {
        Self { data, record_type }
    }

    pub fn record_type(&self) -> TlsRecordType {
        self.record_type
    }

    fn extract_sni(&self) -> Option<&[u8]> {
        if self.record_type != TlsRecordType::Handshake {
            return None;
        }
        let d = &self.data;
        if d.len() < 4 {
            return None;
        }
        if d[0] != 0x01 {
            return None;
        }
        let _handshake_len = ((d[1] as usize) << 16) | ((d[2] as usize) << 8) | (d[3] as usize);
        let mut pos = 4;

        if d.len() < pos + 2 + 32 {
            return None;
        }
        pos += 2 + 32;

        if d.len() < pos + 1 {
            return None;
        }
        let session_id_len = d[pos] as usize;
        pos += 1;

        if d.len() < pos + session_id_len {
            return None;
        }
        pos += session_id_len;

        if d.len() < pos + 2 {
            return None;
        }
        let cipher_suites_len = ((d[pos] as usize) << 8) | (d[pos + 1] as usize);
        pos += 2;

        if d.len() < pos + cipher_suites_len {
            return None;
        }
        pos += cipher_suites_len;

        if d.len() < pos + 1 {
            return None;
        }
        let compression_len = d[pos] as usize;
        pos += 1;

        if d.len() < pos + compression_len {
            return None;
        }
        pos += compression_len;

        if d.len() < pos + 2 {
            return None;
        }
        let extensions_len = ((d[pos] as usize) << 8) | (d[pos + 1] as usize);
        pos += 2;

        let extensions_end = pos + extensions_len;
        if d.len() < extensions_end {
            return None;
        }

        while pos + 4 <= extensions_end {
            let ext_type = ((d[pos] as u16) << 8) | (d[pos + 1] as u16);
            let ext_data_len = ((d[pos + 2] as usize) << 8) | (d[pos + 3] as usize);
            pos += 4;

            if pos + ext_data_len > extensions_end {
                return None;
            }

            if ext_type == 0x0000 {
                if ext_data_len < 2 {
                    return None;
                }
                let list_len = ((d[pos] as usize) << 8) | (d[pos + 1] as usize);
                let mut list_pos = pos + 2;
                let list_end = list_pos + list_len;

                if list_end > pos + ext_data_len {
                    return None;
                }

                while list_pos + 3 <= list_end {
                    let name_type = d[list_pos];
                    let name_len = ((d[list_pos + 1] as usize) << 8) | (d[list_pos + 2] as usize);
                    list_pos += 3;

                    if list_pos + name_len > list_end {
                        return None;
                    }

                    if name_type == 0x00 {
                        return Some(&d[list_pos..list_pos + name_len]);
                    }

                    list_pos += name_len;
                }
                return None;
            }

            pos += ext_data_len;
        }

        None
    }
}

impl Frame for TlsEncryptedFrame {
    fn as_bytes(&self) -> &[u8] { &self.data }
    fn into_bytes(self) -> Bytes { self.data }
    fn routing_key(&self) -> Option<&[u8]> { self.extract_sni() }
}
