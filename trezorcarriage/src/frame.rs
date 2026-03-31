use bytes::Bytes;
use train_track::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsRecordType {
    ChangeCipherSpec,
    Alert,
    Handshake,
    ApplicationData,
}

pub struct TlsEncryptedFrame {
    pub(crate) data: Bytes,
    pub(crate) record_type: TlsRecordType,
}

impl Frame for TlsEncryptedFrame {
    fn as_bytes(&self) -> &[u8] { &self.data }
    fn into_bytes(self) -> Bytes { self.data }
    fn routing_key(&self) -> Option<&[u8]> { None }
}
