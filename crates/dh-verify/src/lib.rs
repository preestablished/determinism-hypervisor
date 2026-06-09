#![forbid(unsafe_code)]

pub const VERIFY_COMPONENT: &str = "dh-verify";

pub fn snapshot_format_version() -> u32 {
    dh_snapshot::DHSNAP_FORMAT_VERSION
}
