#![deny(unsafe_code)] // targeted allows in counter.rs (perf_event_open, ioctl, read)

pub mod counter;

pub const DET_CLOCK_COMPONENT: &str = "dh-detclock";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GuestIcount(pub u64);
