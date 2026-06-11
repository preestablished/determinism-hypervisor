//! Replay executor (bead 39w): drive a run from `(snapshot, DHILOG)` —
//! the product's core property made executable. Restore the snapshot,
//! walk the log's canonical records, land each at its recorded icount
//! via the boundary engine and apply it through the SAME paired
//! `DeviceRail` entry points recording used, feed entropy from the
//! restored ENTR state, verify every EPOCH_HASH record against the live
//! chain as it goes, and check `end_state_hash` at END.
//!
//! QUANTIZATION INDEPENDENCE: replay quantizes BY RECORD (one run
//! quantum per canonical record, plus a final one to `end_icount`) —
//! deliberately different from however the recording quantized its run.
//! The epoch grid is ABSOLUTE in counter space (run_segment_with_epochs'
//! documented contract), so the EPOCH_HASH sets still match link for
//! link; a divergence is reported with the first mismatching epoch.
//!
//! COUNTER AXIS: replay restores with `counter: Some` (§3.1 — the
//! segment counts from zero), which is the axis the recording's icounts
//! were stamped on (a production segment starts at a fresh counter).
//!
//! THE RESEAL HAMMER: replay records through its own rail exactly as
//! the original run did, so on success the resealed log is BYTE-
//! IDENTICAL to the input — the strongest equality this layer can
//! state, subsuming the per-record checks (which exist for granular
//! divergence reporting, not extra strength).
//!
//! Phase-1 scope, loud where cut: DEV_EVENT replay needs the detchannel
//! composition (slot manager, ol1); vectored inputs (a PAD_SET/NET_RX
//! whose device queued an edge interrupt) need run control's injection
//! scheduling contract — both error as `NotYetWired`, never silently
//! skip. The M5 demo path (polling pad-echo, loopback net) needs
//! neither.

use dh_detclock::counter::InstRetired;
use dh_devices::ctx::GuestMem;
use dh_inputlog::reader::{LogReader, ReadError, RecordBody};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::MachineConfig;
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::SlotVm;
use dh_vmm::recording::{DeviceRail, RecordError};
use dh_vmm::runctl::{run_segment_with_epochs, RunError, Segment, StopReason, Until};
use dh_vmm::SlotState;
use snapstore_client::blocking::SnapstoreClient;
use snapstore_types::SnapshotRef;
use std::sync::atomic::AtomicBool;

use crate::restore_engine::{restore_snapshot, RestoreError};

#[derive(Debug)]
pub enum ReplayError {
    Restore(RestoreError),
    Log(ReadError),
    /// The log's header does not belong to this (snapshot, config) pair.
    HeaderMismatch(&'static str),
    /// The replayed machine diverged from the recording. The first
    /// mismatch is reported; nothing after it is trustworthy.
    Divergence {
        what: &'static str,
        at_icount: u64,
        expected: [u8; 32],
        got: [u8; 32],
    },
    /// A canonical record kind this executor cannot apply yet — loud,
    /// never skipped (a skipped input IS a divergence).
    NotYetWired(&'static str),
    Apply(String),
    Run(String),
}

pub struct ReplayOutcome {
    pub records_applied: u64,
    pub epoch_hashes_verified: u64,
    pub end_icount: u64,
    pub end_state_hash: [u8; 32],
    /// The resealed log produced by the replay's own rail — byte-
    /// identical to the input on success (asserted before returning).
    pub resealed: Vec<u8>,
}

/// Replay one sealed segment from its base snapshot. `slot` and `bus`
/// are FRESH (the restore engine's preconditions); the counter is
/// routed to this thread and will be reset by the restore (§3.1).
#[allow(clippy::too_many_arguments)]
pub fn replay_segment<M: GuestMem>(
    slot: &mut SlotVm,
    rail: DeviceRail<M>,
    machine_config: &MachineConfig,
    base_snapshot: SnapshotRef,
    counter: &InstRetired,
    store: &SnapstoreClient,
    log_bytes: &[u8],
) -> Result<ReplayOutcome, ReplayError> {
    let log = LogReader::parse(log_bytes).map_err(ReplayError::Log)?;
    let header = log.header();

    // ── Header ↔ (snapshot, config) identity ─────────────────────────────
    if header.base_snapshot_id != base_snapshot.to_bytes() {
        return Err(ReplayError::HeaderMismatch("base_snapshot_id"));
    }
    let config_hash = machine_config
        .config_hash()
        .map_err(|e| ReplayError::Apply(format!("config hash: {e:?}")))?;
    if header.machine_config_hash != config_hash {
        return Err(ReplayError::HeaderMismatch("machine_config_hash"));
    }
    if header.clock_num != machine_config.clock.num()
        || header.clock_den != machine_config.clock.den()
    {
        return Err(ReplayError::HeaderMismatch("clock ratio"));
    }

    // ── Restore the base (counter reset = the recording's icount axis) ───
    // INTO THE RAIL'S BUS: the rail services every exit, so the restored
    // device state must live in the bus the rail dispatches to — a
    // separate restore bus would leave the rail running default devices
    // (the iteration-88 design catch).
    let mut rail = rail;
    let restored = restore_snapshot(
        slot,
        SlotState::Paused,
        &mut rail.bus,
        machine_config,
        base_snapshot,
        Some(counter),
        None,
        store,
    )
    .map_err(ReplayError::Restore)?;
    // §3.1: zero seed ⇒ continue the snapshot's PRNG; nonzero ⇒ fresh.
    rail.entropy = if header.entropy_seed == [0u8; 32] {
        restored.entropy
    } else {
        dh_devices::entropy::DetEntropy::from_seed(header.entropy_seed)
    };
    let mut chain = restored.chain;
    let rail = std::cell::RefCell::new(rail);

    // Expected epoch hashes, in order, from the recording.
    let expected_epochs: Vec<(u64, u64, [u8; 32])> = log
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::EpochHash {
                epoch_index,
                chain_value,
            } => Some((epoch_index, rec.icount(), chain_value)),
            _ => None,
        })
        .collect();

    let pause = AtomicBool::new(false);
    let mut records_applied = 0u64;

    // One run quantum to `target` (absolute), servicing exits through the
    // rail. Each epoch link is verified against the recording AT THE
    // LINK POINT (the sink) and re-landed in the replay's own log; a
    // mismatch aborts the quantum loudly through the sink error.
    let verified = std::cell::Cell::new(0usize);
    let run_to =
        |slot: &mut SlotVm, chain: &mut StateHashChain, target: u64| -> Result<(), ReplayError> {
            let start = counter
                .read()
                .map_err(|e| ReplayError::Run(format!("{e:?}")))?;
            if target < start {
                return Err(ReplayError::Apply(format!(
                    "record icount {target} is behind the replay position {start} — \
                 records must be monotone"
                )));
            }
            if target == start {
                return Ok(());
            }
            let out = {
                let mut seg = Segment {
                    slot,
                    counter,
                    chain,
                    config: machine_config,
                    start_icount: start,
                    injections: &[],
                    timer: None,
                    pause: &pause,
                };
                run_segment_with_epochs(
                    &mut seg,
                    Until::IcountBudget(target - start),
                    &mut || false,
                    &mut |exit| {
                        let icount = counter
                            .read()
                            .map_err(|e| BoundaryError::Exit(format!("counter: {e:?}")))?;
                        rail.borrow_mut().service_exit(icount, exit)
                    },
                    &mut |idx, icount, value| {
                        let i = verified.get();
                        match expected_epochs.get(i) {
                            Some((e_idx, e_icount, e_value))
                                if *e_idx == idx && *e_icount == icount && *e_value == value => {}
                            Some((_, _, e_value)) => {
                                return Err(BoundaryError::Exit(format!(
                                    "EPOCH_HASH divergence at icount {icount} (epoch {idx}): \
                                 expected {:02x?}.., got {:02x?}..",
                                    &e_value[..4],
                                    &value[..4]
                                )));
                            }
                            None => {
                                return Err(BoundaryError::Exit(format!(
                                    "replay produced an EPOCH_HASH at icount {icount} the \
                                 recording does not have"
                                )));
                            }
                        }
                        verified.set(i + 1);
                        rail.borrow_mut()
                            .log_epoch_hash(idx, icount, value)
                            .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))
                    },
                )
                .map_err(|e: RunError| match e {
                    RunError::Boundary(BoundaryError::Exit(m)) if m.contains("EPOCH_HASH") => {
                        ReplayError::Divergence {
                            what: "EPOCH_HASH (see message)",
                            at_icount: start,
                            expected: [0; 32],
                            got: [0; 32],
                        }
                    }
                    other => ReplayError::Run(format!("{other}")),
                })?
            };
            if out.reason != StopReason::BudgetReached {
                return Err(ReplayError::Run(format!(
                    "expected to land at {target}, stopped {:?} at {}",
                    out.reason, out.boundary.icount
                )));
            }
            Ok(())
        };

    // ── Walk the canonical records ────────────────────────────────────────
    for rec in log.canonical() {
        let icount = rec.icount();
        let rip = rec.boundary_rip();
        match rec.body() {
            RecordBody::End { .. } => break, // handled after the loop
            RecordBody::PadSet {
                port,
                buttons,
                frame_hint,
            } => {
                run_to(slot, &mut chain, icount)?;
                rail.borrow_mut()
                    .apply_pad_set(icount, rip, port, buttons, frame_hint)
                    .map_err(|e: RecordError| ReplayError::Apply(format!("{e:?}")))?;
            }
            RecordBody::NetRx { frame } => {
                run_to(slot, &mut chain, icount)?;
                rail.borrow_mut()
                    .apply_net_rx(icount, rip, frame)
                    .map_err(|e: RecordError| ReplayError::Apply(format!("{e:?}")))?;
            }
            RecordBody::DevEvent { .. } => {
                return Err(ReplayError::NotYetWired(
                    "DEV_EVENT replay needs the detchannel composition (ol1)",
                ));
            }
            other => {
                return Err(ReplayError::Apply(format!(
                    "unexpected canonical record in v1 replay: {other:?}"
                )));
            }
        }
        if !rail.borrow().irqs.is_empty() {
            return Err(ReplayError::NotYetWired(
                "vectored input replay needs run control's injection scheduling",
            ));
        }
        records_applied += 1;
    }

    // ── Run out the tail and check the END identity ───────────────────────
    run_to(slot, &mut chain, header.end_icount)?;
    let verified = verified.get();
    if verified != expected_epochs.len() {
        return Err(ReplayError::Divergence {
            what: "EPOCH_HASH count (recording has more than replay produced)",
            at_icount: header.end_icount,
            expected: [0; 32],
            got: [0; 32],
        });
    }
    let live_end = chain.value();
    if live_end != header.end_state_hash {
        return Err(ReplayError::Divergence {
            what: "end_state_hash",
            at_icount: header.end_icount,
            expected: header.end_state_hash,
            got: live_end,
        });
    }

    // ── The reseal hammer ─────────────────────────────────────────────────
    let (stop_reason, _) = log.end();
    let outcome_like = dh_vmm::runctl::SegmentOutcome {
        reason: stop_reason_from_u8(stop_reason)?,
        boundary: dh_vmm::boundary::Boundary {
            icount: header.end_icount,
            rip: 0,
            rcx: 0,
        },
        vns: header.end_vns,
        state_hash: live_end,
        injections_delivered: 0,
        timer_fired: None,
    };
    let resealed = rail
        .into_inner()
        .seal(&outcome_like, header.end_snapshot_id)
        .map_err(|e| ReplayError::Apply(format!("reseal: {e:?}")))?;
    if resealed != log_bytes {
        return Err(ReplayError::Divergence {
            what: "resealed log bytes",
            at_icount: header.end_icount,
            expected: header.body_hash,
            got: [0; 32], // byte-compare failed; the diff is in the logs
        });
    }

    Ok(ReplayOutcome {
        records_applied,
        epoch_hashes_verified: verified as u64,
        end_icount: header.end_icount,
        end_state_hash: live_end,
        resealed,
    })
}

/// Inverse of `dh_vmm::recording::stop_reason_u8` for the END byte —
/// total over the recorded subset; unknown bytes are a log fault.
fn stop_reason_from_u8(b: u8) -> Result<StopReason, ReplayError> {
    Ok(match b {
        1 => StopReason::BudgetReached,
        2 => StopReason::GoalSatisfied,
        4 => StopReason::HardCap,
        5 => StopReason::Paused,
        6 => StopReason::GuestHalted,
        _ => return Err(ReplayError::Apply(format!("unknown END stop_reason {b}"))),
    })
}
