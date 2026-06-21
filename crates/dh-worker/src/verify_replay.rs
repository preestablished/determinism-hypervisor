//! VerifyReplay execution (bead 1py): run the replay engine over a
//! (snapshot, DHILOG) pair and report through dh-verify's model —
//! `EpochOk` per matched epoch, `VerifyDone` on a full match,
//! `Divergence` (first bad epoch + hash pair) otherwise. The M6 RPC
//! (rfv) streams these; the icount-range bisection is M8.
//!
//! The replay engine verifies every EPOCH_HASH at the link point and
//! calls this wrapper as each one matches; the wrapper translates those
//! live callbacks plus the terminal replay outcome into the reporting
//! model. On divergence the engine's structured report maps 1:1.

use dh_detclock::counter::InstRetired;
use dh_devices::ctx::GuestMem;
use dh_verify::verify::{VerifyProgress, VerifyReport};
use dh_vmm::config::MachineConfig;
use dh_vmm::kvm::SlotVm;
use dh_vmm::recording::DeviceRail;
use snapstore_client::blocking::SnapstoreClient;
use snapstore_types::SnapshotRef;

use crate::bisection_index::BisectionCheckpointIndex;
use crate::replay_engine::{replay_segment_with_epoch_progress_and_bisection, ReplayError};

/// Execute and verify. `Ok(report)` always carries either a `Done` or a
/// `Divergence` event; infrastructure failures (store, log parse, KVM)
/// stay errors — they are not verdicts about the recording.
#[allow(clippy::too_many_arguments)]
pub fn verify_replay<M>(
    slot: &mut SlotVm,
    rail: DeviceRail<M>,
    machine_config: &MachineConfig,
    base_snapshot: SnapshotRef,
    counter: &InstRetired,
    store: &SnapstoreClient,
    log_bytes: &[u8],
) -> Result<VerifyReport, ReplayError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
{
    let mut report = VerifyReport::default();
    let terminal = verify_replay_with_progress(
        slot,
        rail,
        machine_config,
        base_snapshot,
        counter,
        store,
        log_bytes,
        |event| {
            report.push(event);
            Ok(())
        },
    )?;
    report.push(terminal);
    Ok(report)
}

/// Streaming verifier entry point: emits each verified epoch through
/// `on_progress` as replay reaches it, then returns exactly one terminal
/// event (`Done` or `Divergence`).
#[allow(clippy::too_many_arguments)]
pub fn verify_replay_with_progress<M, F>(
    slot: &mut SlotVm,
    rail: DeviceRail<M>,
    machine_config: &MachineConfig,
    base_snapshot: SnapshotRef,
    counter: &InstRetired,
    store: &SnapstoreClient,
    log_bytes: &[u8],
    on_progress: F,
) -> Result<VerifyProgress, ReplayError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
    F: FnMut(VerifyProgress) -> Result<(), ReplayError>,
{
    verify_replay_with_bisection_progress(
        slot,
        rail,
        machine_config,
        base_snapshot,
        counter,
        store,
        log_bytes,
        None,
        on_progress,
    )
}

/// Streaming verifier entry point with optional M8 bisection refinement.
#[allow(clippy::too_many_arguments)]
pub fn verify_replay_with_bisection_progress<M, F>(
    slot: &mut SlotVm,
    rail: DeviceRail<M>,
    machine_config: &MachineConfig,
    base_snapshot: SnapshotRef,
    counter: &InstRetired,
    store: &SnapstoreClient,
    log_bytes: &[u8],
    bisection_index: Option<&BisectionCheckpointIndex>,
    mut on_progress: F,
) -> Result<VerifyProgress, ReplayError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
    F: FnMut(VerifyProgress) -> Result<(), ReplayError>,
{
    match replay_segment_with_epoch_progress_and_bisection(
        slot,
        rail,
        machine_config,
        base_snapshot,
        counter,
        store,
        log_bytes,
        bisection_index,
        |epoch_index, icount, _chain_value| {
            on_progress(VerifyProgress::EpochOk {
                epoch_index,
                icount,
            })
        },
    ) {
        Ok(outcome) => Ok(VerifyProgress::Done {
            total_icount: outcome.end_icount,
            end_state_hash: outcome.end_state_hash,
        }),
        Err(ReplayError::Divergence {
            what,
            at_icount,
            expected,
            got,
        }) => Ok(VerifyProgress::Divergence {
            first_bad_epoch: first_bad_epoch_for(&what, at_icount, machine_config.epoch_len),
            at_icount,
            what,
            expected,
            got,
        }),
        Err(ReplayError::BisectionDivergence(divergence)) => {
            Ok(VerifyProgress::BisectionDivergence(divergence))
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
