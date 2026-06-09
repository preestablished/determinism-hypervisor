#![forbid(unsafe_code)]

use dh_types::SlotState;

pub fn initial_slot_state() -> SlotState {
    SlotState::Empty
}
