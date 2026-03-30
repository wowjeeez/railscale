use bytes::Bytes;

pub trait Frame: Send + Sized + Sync {
    fn as_bytes(&self) -> &[u8];
    fn into_bytes(self) -> Bytes;
    fn routing_key(&self) -> Option<&[u8]>;
}

pub enum ParsedData<F: Frame> {
    Parsed(F),
    Passthrough(Bytes),
}
