#![forbid(unsafe_code)]

pub const DHILOG_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputEvent {
    pub guest_icount: u64,
    pub payload: Vec<u8>,
}

pub fn payload_digest(payload: &[u8]) -> [u8; 32] {
    *blake3::hash(payload).as_bytes()
}
