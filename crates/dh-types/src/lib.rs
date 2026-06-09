#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    Empty,
    Running,
    Paused,
    Frozen,
}
