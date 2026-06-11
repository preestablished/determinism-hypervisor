//! VerifyReplay execution (bead 1py): run the replay engine over a
//! (snapshot, DHILOG) pair and report through dh-verify's model —
//! `EpochOk` per matched epoch, `VerifyDone` on a full match,
//! `Divergence` (first bad epoch + hash pair) otherwise. The M6 RPC
//! (rfv) streams these; the icount-range bisection is M8.
//!
//! The replay engine already verifies every EPOCH_HASH at the link
//! point and fails fast — this wrapper translates its outcome into the
//! reporting model. On success the EpochOk stream is reconstructed from
//! the log's own records (every one of which the replay proved); on
//! divergence the engine's structured report maps 1:1.

use dh_detclock::counter::InstRetired;
use dh_devices::ctx::GuestMem;
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_verify::verify::{VerifyProgress, VerifyReport};
use dh_vmm::config::MachineConfig;
use dh_vmm::kvm::SlotVm;
use dh_vmm::recording::DeviceRail;
use snapstore_client::blocking::SnapstoreClient;
use snapstore_types::SnapshotRef;

use crate::replay_engine::{replay_segment, ReplayError};

/// Execute and verify. `Ok(report)` always carries either a `Done` or a
/// `Divergence` event; infrastructure failures (store, log parse, KVM)
/// stay errors — they are not verdicts about the recording.
#[allow(clippy::too_many_arguments)]
pub fn verify_replay<M: GuestMem>(
    slot: &mut SlotVm,
    rail: DeviceRail<M>,
    machine_config: &MachineConfig,
    base_snapshot: SnapshotRef,
    counter: &InstRetired,
    store: &SnapstoreClient,
    log_bytes: &[u8],
) -> Result<VerifyReport, ReplayError> {
    let mut report = VerifyReport::default();
    match replay_segment(
        slot,
        rail,
        machine_config,
        base_snapshot,
        counter,
        store,
        log_bytes,
    ) {
        Ok(outcome) => {
            // Every EPOCH_HASH in the log was verified at its link point
            // (the engine fails fast and pins the count) — reconstruct
            // the EpochOk stream from the records themselves.
            let log = LogReader::parse(log_bytes).map_err(ReplayError::Log)?;
            let mut emitted = 0u64;
            for rec in log.aux() {
                if let RecordBody::EpochHash { epoch_index, .. } = rec.body() {
                    report.push(VerifyProgress::EpochOk {
                        epoch_index,
                        icount: rec.icount(),
                    });
                    emitted += 1;
                }
            }
            // Structurally impossible to diverge (the engine pins the
            // count before Ok and the parser forbids smuggled records —
            // iteration-89 review traced it) — but a silent release-mode
            // misreport is the one failure this layer exists to prevent,
            // so the check is HARD (review I3).
            if emitted != outcome.epoch_hashes_verified {
                return Err(ReplayError::Apply(format!(
                    "EpochOk reconstruction mismatch: log carries {emitted}, \
                     engine verified {}",
                    outcome.epoch_hashes_verified
                )));
            }
            report.push(VerifyProgress::Done {
                total_icount: outcome.end_icount,
                end_state_hash: outcome.end_state_hash,
            });
            Ok(report)
        }
        Err(ReplayError::Divergence {
            what,
            at_icount,
            expected,
            got,
        }) => {
            report.push(VerifyProgress::Divergence {
                first_bad_epoch: first_bad_epoch_for(what, at_icount, machine_config.epoch_len),
                at_icount,
                what,
                expected,
                got,
            });
            Ok(report)
        }
        Err(other) => Err(other),
    }
}

/// `first_bad_epoch` is meaningful ONLY when an epoch link itself
/// diverged. END-identity kinds (end_state_hash, end_vns, epoch count)
/// happen AFTER every epoch matched, and the reseal kind's `at_icount`
/// is a byte offset — naming an epoch in those cases would blame one
/// that verified (iteration-89 review I1).
fn first_bad_epoch_for(what: &str, at_icount: u64, epoch_len: u64) -> Option<u64> {
    if what.starts_with("EPOCH_HASH chain value")
        || what.starts_with("EPOCH_HASH the recording does not have")
    {
        Some(at_icount / epoch_len.max(1))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::first_bad_epoch_for;

    #[test]
    fn epoch_attribution_is_what_aware() {
        assert_eq!(
            first_bad_epoch_for("EPOCH_HASH chain value", 60_000, 30_000),
            Some(2)
        );
        assert_eq!(
            first_bad_epoch_for("EPOCH_HASH the recording does not have", 30_000, 30_000),
            Some(1)
        );
        // END-identity kinds: every epoch matched — no epoch to blame.
        assert_eq!(first_bad_epoch_for("end_state_hash", 300_000, 30_000), None);
        assert_eq!(first_bad_epoch_for("end_vns", 300_000, 30_000), None);
        assert_eq!(
            first_bad_epoch_for(
                "EPOCH_HASH count (recording has more than replay produced)",
                300_000,
                30_000
            ),
            None
        );
        // Reseal: at_icount is a BYTE OFFSET — an epoch index from it
        // would be nonsense.
        assert_eq!(
            first_bad_epoch_for(
                "resealed log bytes (at_icount = first differing byte offset)",
                123,
                30_000
            ),
            None
        );
    }
}
