//! Domain ↔ proto enum bridges (bead sr5). HAND-WRITTEN MATCHES ONLY.
//!
//! The two enum families disagree in offset AND order — domain
//! `SlotState::Running` is discriminant 1 while proto `RUNNING = 3`, and
//! proto reserves 0 for `*_UNSPECIFIED` which the domain enums never
//! carry — so an `as i32` cast on a domain enum is ALWAYS a bug. Every
//! crossing goes through these functions; the unit tests pin each arm to
//! the exact proto wire number so a renumbering on either side breaks
//! loudly here instead of silently mislabeling slots on the API.
//!
//! Exhaustiveness is the other half of the contract: when run control
//! grows `runctl::StopReason::Faulted` / `NextSdkEvent` producers, the
//! match below stops compiling and forces the mapping decision at the
//! same commit — never a silent `_ => Unspecified`.

use dh_proto::v1 as proto;
use dh_vmm::SlotState;

/// Slot lifecycle → API.md §2.8 `SlotState` (the `_S` suffixes are the
/// proto package's enum-value collision convention, not semantics).
pub fn slot_state_to_proto(s: SlotState) -> proto::SlotState {
    match s {
        SlotState::Empty => proto::SlotState::Empty,
        SlotState::Paused => proto::SlotState::PausedS,
        SlotState::Running => proto::SlotState::Running,
        SlotState::Frozen => proto::SlotState::Frozen,
        SlotState::Faulted => proto::SlotState::FaultedS,
    }
}

/// Segment stop → API.md §2.4 `StopReason`. runctl deliberately has no
/// `NextSdkEvent` (NotYetWired until the device run loop) and no
/// `Faulted` (fault wiring is run control's, see sr5 notes) — those
/// arms appear here the day the variants do.
#[cfg(target_arch = "x86_64")]
pub fn stop_reason_to_proto(r: dh_vmm::runctl::StopReason) -> proto::StopReason {
    use dh_vmm::runctl::StopReason as R;
    match r {
        R::BudgetReached => proto::StopReason::BudgetReached,
        R::GoalSatisfied => proto::StopReason::GoalSatisfied,
        R::HardCap => proto::StopReason::HardCap,
        R::Paused => proto::StopReason::Paused,
        R::GuestHalted => proto::StopReason::GuestHalted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every arm pinned to the exact wire number — the order-divergence
    /// trap (domain Running=1 vs proto RUNNING=3) is the reason this
    /// module exists.
    #[test]
    fn slot_state_wire_numbers_are_pinned() {
        let pins = [
            (SlotState::Empty, 1),
            (SlotState::Paused, 2),
            (SlotState::Running, 3),
            (SlotState::Frozen, 4),
            (SlotState::Faulted, 5),
        ];
        for (domain, wire) in pins {
            assert_eq!(
                slot_state_to_proto(domain) as i32,
                wire,
                "{domain:?} wire number"
            );
        }
        // The trap itself, demonstrated: the naive cast lies for four of
        // the five states (it agrees on Paused=2 by pure coincidence —
        // which is exactly what makes the bug class survive spot checks).
        let lying_casts = pins.iter().filter(|(d, w)| (*d as i32) != *w).count();
        assert_eq!(lying_casts, 4, "the order-divergence trap moved");
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn stop_reason_wire_numbers_are_pinned() {
        use dh_vmm::runctl::StopReason as R;
        let pins = [
            (R::BudgetReached, 1),
            (R::GoalSatisfied, 2),
            (R::HardCap, 4),
            (R::Paused, 5),
            (R::GuestHalted, 6),
        ];
        for (domain, wire) in pins {
            assert_eq!(
                stop_reason_to_proto(domain) as i32,
                wire,
                "{domain:?} wire number"
            );
        }
    }
}
