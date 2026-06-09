#![forbid(unsafe_code)]

pub const DEVICE_MODEL_COMPONENT: &str = "dh-devices";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MmioRange {
    pub base: u64,
    pub len: u64,
}

pub trait DetDevice {
    fn range(&self) -> MmioRange;
    fn snapshot_label(&self) -> &'static str;
}

pub fn input_payload_digest(payload: &[u8]) -> [u8; 32] {
    dh_inputlog::payload_digest(payload)
}
