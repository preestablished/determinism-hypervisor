#![forbid(unsafe_code)]

pub const DET_CLOCK_COMPONENT: &str = "dh-detclock";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GuestIcount(pub u64);
