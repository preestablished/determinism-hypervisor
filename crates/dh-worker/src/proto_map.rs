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
//! grows a `runctl::StopReason::Faulted` producer, the match below stops
//! compiling and forces the mapping decision at the same commit — never
//! a silent `_ => Unspecified`. (`NextSdkEvent` landed exactly that way
//! with bead 4qo.)

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

/// Slot-manager lease → wire `Lease` (API.md §2.1).
pub fn lease_to_proto(l: &crate::slot_manager::Lease) -> proto::Lease {
    proto::Lease {
        slot_id: l.slot_id,
        token: l.token.to_vec(),
    }
}

/// Slot-manager introspection row → wire `SlotInfo` (API.md §2.8). The
/// state field is a prost open-enum i32 — `i32::from` on the PROTO enum
/// (never a domain cast; the deny test below holds the line).
pub fn slot_info_to_proto(i: &crate::slot_manager::SlotInfo) -> proto::SlotInfo {
    proto::SlotInfo {
        slot_id: i.slot_id,
        state: i32::from(slot_state_to_proto(i.state)),
        icount: i.icount,
        base: i.base_snapshot_id.map(|hash| proto::SnapshotRef {
            hash: hash.to_vec(),
        }),
        live_children: i.live_children,
    }
}

/// Segment stop → API.md §2.4 `StopReason`. runctl deliberately has no
/// `Faulted` (fault wiring is run control's, see sr5 notes) — that arm
/// appears here the day the variant does.
#[cfg(target_arch = "x86_64")]
pub fn stop_reason_to_proto(r: dh_vmm::runctl::StopReason) -> proto::StopReason {
    use dh_vmm::runctl::StopReason as R;
    match r {
        R::BudgetReached => proto::StopReason::BudgetReached,
        R::GoalSatisfied => proto::StopReason::GoalSatisfied,
        R::NextSdkEvent => proto::StopReason::NextSdkEvent,
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

    /// Cross-pin: the DHILOG END byte (dh_vmm::recording::stop_reason_u8,
    /// which cannot see dh-proto) must agree with the proto mapping for
    /// every variant — the §3.3 "mirrors proto StopReason" coupling.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn recording_end_byte_agrees_with_the_proto_mapping() {
        use dh_vmm::runctl::StopReason as R;
        for r in [
            R::BudgetReached,
            R::GoalSatisfied,
            R::NextSdkEvent,
            R::HardCap,
            R::Paused,
            R::GuestHalted,
        ] {
            assert_eq!(
                i32::from(dh_vmm::recording::stop_reason_u8(r)),
                stop_reason_to_proto(r) as i32,
                "{r:?}"
            );
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn stop_reason_wire_numbers_are_pinned() {
        use dh_vmm::runctl::StopReason as R;
        let pins = [
            (R::BudgetReached, 1),
            (R::GoalSatisfied, 2),
            (R::NextSdkEvent, 3),
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

    /// The §2.8 introspection crossings carry every field, and the state
    /// travels through the pinned mapping (never a domain cast).
    #[test]
    fn slot_info_and_lease_cross_with_every_field() {
        let lease = crate::slot_manager::Lease {
            slot_id: 3,
            token: [0xA5; 16],
        };
        let wire = lease_to_proto(&lease);
        assert_eq!(wire.slot_id, 3);
        assert_eq!(wire.token, vec![0xA5; 16]);

        let info = crate::slot_manager::SlotInfo {
            slot_id: 2,
            state: SlotState::Frozen,
            icount: 7_000_000,
            base_snapshot_id: Some([0xBC; 32]),
            live_children: 4,
        };
        let wire = slot_info_to_proto(&info);
        assert_eq!(wire.slot_id, 2);
        assert_eq!(wire.state, 4, "Frozen through the pinned mapping");
        assert_eq!(wire.icount, 7_000_000);
        assert_eq!(wire.base.as_ref().unwrap().hash, vec![0xBC; 32]);
        assert_eq!(wire.live_children, 4);

        let bare = crate::slot_manager::SlotInfo {
            base_snapshot_id: None,
            ..info
        };
        assert_eq!(slot_info_to_proto(&bare).base, None, "no segment yet");
    }

    /// The iteration-80 deny gate (sr5 review, armed by ol1's SlotInfo
    /// serving): a domain-enum `as i32` silently mislabels four of five
    /// slot states on the wire, so OUTSIDE this module no dh-worker
    /// source may contain `as i32` at all — every enum crossing comes
    /// here. (Same shape as dh-devices' no_host_ambient_authority gate.)
    #[test]
    fn no_enum_casts_outside_proto_map() {
        fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, hits);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") || path.ends_with("proto_map.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (lineno, line) in text.lines().enumerate() {
                    if line.contains("as i32") {
                        hits.push(format!("{}:{}: {line}", path.display(), lineno + 1));
                    }
                }
            }
        }
        let mut hits = Vec::new();
        walk(
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
            &mut hits,
        );
        assert!(
            hits.is_empty(),
            "`as i32` outside proto_map.rs — domain enums must cross through \
             the pinned mappings (SlotState::Running as i32 == 1, proto \
             RUNNING == 3):\n{}",
            hits.join("\n")
        );
    }
}
