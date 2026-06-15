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
//! Phase-1 scope, loud where cut: DEV_EVENT records replay through the
//! generic device-event rail; vectored inputs (a PAD_SET/NET_RX whose
//! device queued an edge interrupt) still need run control's injection
//! scheduling contract and error as `NotYetWired`, never silently skip.
//! The M5 demo path (polling pad-echo, loopback net) needs no vectors.

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

/// The structured divergence captured by the epoch sink:
/// `(what, at_icount, expected, got)`.
type DivergenceCell = std::cell::Cell<Option<(&'static str, u64, [u8; 32], [u8; 32])>>;

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
    let divergence: DivergenceCell = std::cell::Cell::new(None);
    let run_to = |slot: &mut SlotVm,
                  chain: &mut StateHashChain,
                  target: u64|
     -> Result<Option<dh_vmm::runctl::SegmentOutcome>, ReplayError> {
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
            return Ok(None);
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
                sdk_events: None,
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
                            divergence.set(Some((
                                "EPOCH_HASH chain value",
                                icount,
                                *e_value,
                                value,
                            )));
                            return Err(BoundaryError::Exit("epoch divergence".into()));
                        }
                        None => {
                            divergence.set(Some((
                                "EPOCH_HASH the recording does not have",
                                icount,
                                [0; 32],
                                value,
                            )));
                            return Err(BoundaryError::Exit("epoch divergence".into()));
                        }
                    }
                    verified.set(i + 1);
                    rail.borrow_mut()
                        .log_epoch_hash(idx, icount, value)
                        .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))
                },
            )
            .map_err(|e: RunError| {
                // Structured side channel (iteration-88 review I1): the
                // sink can only surface a BoundaryError; the real
                // diagnostics travel through the Cell.
                if let Some((what, at_icount, expected, got)) = divergence.take() {
                    ReplayError::Divergence {
                        what,
                        at_icount,
                        expected,
                        got,
                    }
                } else {
                    ReplayError::Run(format!("{e}"))
                }
            })?
        };
        Ok(Some(out))
    };
    // Intermediate quanta must land exactly (BudgetReached at the
    // record's icount); the TAIL has its own contract at the call site.
    let require_landed =
        |out: &Option<dh_vmm::runctl::SegmentOutcome>, target: u64| -> Result<(), ReplayError> {
            match out {
                None => Ok(()),
                Some(o) if o.reason == StopReason::BudgetReached => Ok(()),
                Some(o) => Err(ReplayError::Run(format!(
                    "expected to land at {target}, stopped {:?} at {}",
                    o.reason, o.boundary.icount
                ))),
            }
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
                let o = run_to(slot, &mut chain, icount)?;
                require_landed(&o, icount)?;
                rail.borrow_mut()
                    .apply_pad_set(icount, rip, port, buttons, frame_hint)
                    .map_err(|e: RecordError| ReplayError::Apply(format!("{e:?}")))?;
            }
            RecordBody::NetRx { frame } => {
                let o = run_to(slot, &mut chain, icount)?;
                require_landed(&o, icount)?;
                rail.borrow_mut()
                    .apply_net_rx(icount, rip, frame)
                    .map_err(|e: RecordError| ReplayError::Apply(format!("{e:?}")))?;
            }
            RecordBody::DevEvent {
                device_id,
                event_type,
                data,
            } => {
                let o = run_to(slot, &mut chain, icount)?;
                require_landed(&o, icount)?;
                rail.borrow_mut()
                    .apply_dev_event(icount, rip, device_id, event_type, data)
                    .map_err(|e: RecordError| ReplayError::Apply(format!("{e:?}")))?;
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
    let (stop_reason_byte, _) = log.end();
    let expected_reason = stop_reason_from_u8(stop_reason_byte)?;
    let tail = run_to(slot, &mut chain, header.end_icount)?;
    if let Some(out) = &tail {
        // A GuestHalted recording legitimately stops ON its halt at
        // end_icount; anything else must land the budget exactly. Either
        // way the boundary must BE end_icount (iteration-88 review I2 —
        // the halt coincidence is now a pinned contract, not luck).
        let reason_ok = out.reason == StopReason::BudgetReached
            || (out.reason == StopReason::GuestHalted
                && expected_reason == StopReason::GuestHalted);
        if !reason_ok || out.boundary.icount != header.end_icount {
            return Err(ReplayError::Run(format!(
                "tail stopped {:?} at {} (recording ended {:?} at {})",
                out.reason, out.boundary.icount, expected_reason, header.end_icount
            )));
        }
        // end_vns travels OUTSIDE body_hash (header-only), so the reseal
        // byte-compare cannot verify it — check the live value here
        // (iteration-88 review I1/opus2; masked by 1:1 clocks until a5e).
        if out.vns != header.end_vns {
            return Err(ReplayError::Divergence {
                what: "end_vns",
                at_icount: header.end_icount,
                expected: u64_hash(header.end_vns),
                got: u64_hash(out.vns),
            });
        }
    }
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
    let outcome_like = dh_vmm::runctl::SegmentOutcome {
        reason: expected_reason,
        boundary: dh_vmm::boundary::Boundary {
            icount: header.end_icount,
            rip: 0,
            rcx: 0,
        },
        vns: header.end_vns,
        state_hash: live_end,
        injections_delivered: 0,
        timer_fired: None,
        // seal() does not read frames_elapsed (frame marks travel as AUX
        // FRAME_MARK records, not END fields) — any value reseals the
        // same bytes.
        frames_elapsed: 0,
    };
    let resealed = rail
        .into_inner()
        .seal(&outcome_like, header.end_snapshot_id)
        .map_err(|e| ReplayError::Apply(format!("reseal: {e:?}")))?;
    if resealed != log_bytes {
        // Find the first differing offset for the report (iteration-88
        // review I2 — an undiffable pair helps nobody).
        let first_diff = resealed
            .iter()
            .zip(log_bytes.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| resealed.len().min(log_bytes.len()));
        return Err(ReplayError::Divergence {
            what: "resealed log bytes (at_icount = first differing byte offset)",
            at_icount: first_diff as u64,
            expected: header.body_hash,
            got: [0; 32],
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

/// A u64 packed into the 32-byte divergence slot (LE in the first 8).
fn u64_hash(v: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&v.to_le_bytes());
    out
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
