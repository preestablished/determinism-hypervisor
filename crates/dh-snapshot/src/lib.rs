#![forbid(unsafe_code)]

pub mod dhsnap;

pub const DHSNAP_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirtyPage {
    pub gpa: u64,
    pub bytes: Vec<u8>,
}
