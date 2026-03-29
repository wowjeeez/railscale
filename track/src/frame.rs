use bytes::Bytes;

pub trait Frame: Send + Sized {
    fn as_bytes(&self) -> &[u8];
    fn into_bytes(self) -> Bytes;
    fn is_routing_frame(&self) -> bool;
}

pub enum ParsedData<F: Frame> {
    Parsed(F),
    Passthrough(Bytes),
}
