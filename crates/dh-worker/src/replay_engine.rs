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
//! IDENTICAL to the input, except for diagnostic-only bisection
//! checkpoint AUX records. Those records are evidence captured during
//! recording, not replay inputs, so replay may prove equivalence by
//! matching every non-checkpoint record after sequence renumbering.
//!
//! Phase-1 scope, loud where cut: DEV_EVENT records replay through the
//! generic device-event rail; vectored inputs (a PAD_SET/NET_RX whose
//! device queued an edge interrupt) still need run control's injection
//! scheduling contract and error as `NotYetWired`, never silently skip.
//! The M5 demo path (polling pad-echo, loopback net) needs no vectors.

use dh_detclock::counter::InstRetired;
use dh_devices::ctx::GuestMem;
use dh_inputlog::reader::{Header, LogReader, ReadError, RecordBody};
use dh_verify::verify::{BisectionDivergence, BisectionEvidence, BisectionMode};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::MachineConfig;
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::SlotVm;
use dh_vmm::recording::{DeviceRail, RecordError};
use dh_vmm::runctl::{
    run_segment_with_epoch_options, RunError, RunOptions, Segment, StopReason, Until,
};
use dh_vmm::SlotState;
use snapstore_client::blocking::SnapstoreClient;
use snapstore_types::SnapshotRef;
use std::sync::atomic::AtomicBool;

use crate::bisection_index::{
    BisectionCheckpointIndex, BisectionDivergenceSite, BisectionSelectionTarget,
    IndexedBisectionCheckpoint, RecordPosition, SelectedBisectionCheckpoint,
};
use crate::restore_engine::{restore_snapshot, RestoreError};
use crate::runtime::runtime_hash_device_sections;

/// The structured divergence captured by the epoch sink:
/// `(what, at_icount, expected, got)`.
type DivergenceCell = std::cell::Cell<Option<(&'static str, u64, [u8; 32], [u8; 32])>>;

type ReplayDetChannel<M> = dh_devices::detchannel::DetChannelDevice<
    M,
    detguest_host::LogFaultPlan,
    fn() -> detguest_host::LogFaultPlan,
>;

#[derive(Debug, Default)]
struct ReplayExitEvents {
    sdk_streams: Vec<u32>,
}

fn replay_detchannel_mut<M>(bus: &mut dh_devices::MmioBus) -> Option<&mut ReplayDetChannel<M>>
where
    M: detguest_host::GuestMem + Clone + Send + 'static,
{
    bus.devices_mut().find_map(|(_base, dev)| {
        if dev.device_id() != dh_devices::detchannel::DEVICE_ID_DETCHANNEL {
            return None;
        }
        dev.as_any_mut()?.downcast_mut::<ReplayDetChannel<M>>()
    })
}

fn replay_service_exit<M>(
    rail: &mut DeviceRail<M>,
    icount: u64,
    exit: kvm_ioctls::VcpuExit<'_>,
) -> Result<ReplayExitEvents, BoundaryError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
{
    let serial_end = dh_vmm::kvm::PIO_SERIAL_BASE + dh_vmm::kvm::PIO_SERIAL_LEN;
    let detcall_end = dh_vmm::kvm::PIO_DETCALL_BASE + dh_vmm::kvm::PIO_DETCALL_LEN;
    let mut ctx = dh_devices::DevCtx::new(
        icount,
        0,
        &mut rail.log,
        &mut rail.mem,
        &mut rail.entropy,
        &mut rail.irqs,
    );

    let sdk_streams = match exit {
        kvm_ioctls::VcpuExit::IoOut(port, data)
            if (dh_vmm::kvm::PIO_SERIAL_BASE..serial_end).contains(&port) =>
        {
            rail.serial.pio_write(port, data);
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::IoIn(port, data)
            if (dh_vmm::kvm::PIO_SERIAL_BASE..serial_end).contains(&port) =>
        {
            rail.serial.pio_read(port, data);
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::MmioRead(gpa, data)
            if dh_vmm::lapic::LocalApic::contains_mmio(gpa) =>
        {
            rail.lapic
                .read_mmio(gpa, data)
                .map_err(|e| BoundaryError::Exit(format!("lapic mmio read {gpa:#x}: {e:?}")))?;
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::MmioWrite(gpa, data)
            if dh_vmm::lapic::LocalApic::contains_mmio(gpa) =>
        {
            rail.lapic
                .write_mmio(gpa, data)
                .map_err(|e| BoundaryError::Exit(format!("lapic mmio write {gpa:#x}: {e:?}")))?;
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::X86Rdmsr(msr)
            if dh_vmm::lapic::LocalApic::is_lapic_msr(msr.index) =>
        {
            match rail.lapic.read_msr(msr.index) {
                Ok(value) => {
                    *msr.data = value;
                    *msr.error = 0;
                }
                Err(e) => {
                    *msr.error = 1;
                    return Err(BoundaryError::Exit(format!(
                        "lapic rdmsr {:#x}: {e:?}",
                        msr.index
                    )));
                }
            }
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::X86Wrmsr(msr)
            if dh_vmm::lapic::LocalApic::is_lapic_msr(msr.index) =>
        {
            match rail.lapic.write_msr(msr.index, msr.data) {
                Ok(()) => *msr.error = 0,
                Err(e) => {
                    *msr.error = 1;
                    return Err(BoundaryError::Exit(format!(
                        "lapic wrmsr {:#x}: {e:?}",
                        msr.index
                    )));
                }
            }
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::X86Rdmsr(msr) => {
            match dh_vmm::msr::on_denied_rdmsr(msr.index) {
                dh_vmm::msr::MsrAction::SupplyValue(value) => {
                    *msr.data = value;
                    *msr.error = 0;
                }
                dh_vmm::msr::MsrAction::AckWrite | dh_vmm::msr::MsrAction::InjectGp => {
                    *msr.error = 1;
                }
            }
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::X86Wrmsr(msr) => {
            match dh_vmm::msr::on_denied_wrmsr(msr.index) {
                dh_vmm::msr::MsrAction::SupplyValue(_) | dh_vmm::msr::MsrAction::AckWrite => {
                    *msr.error = 0;
                }
                dh_vmm::msr::MsrAction::InjectGp => {
                    *msr.error = 1;
                }
            }
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::IoOut(port, data)
            if (dh_vmm::kvm::PIO_DETCALL_BASE..detcall_end).contains(&port) =>
        {
            let host = replay_detchannel_mut::<M>(&mut rail.bus).ok_or_else(|| {
                BoundaryError::Exit("detchannel PIO without DetChannelDevice".into())
            })?;
            let mut word = [0u8; 4];
            let n = data.len().min(4);
            word[..n].copy_from_slice(&data[..n]);
            let events = host
                .host_mut()
                .pio_out(port, u32::from_le_bytes(word), &mut ctx);
            if host.host().metrics.any_anomaly() {
                return Err(BoundaryError::Exit("detchannel drain anomaly".into()));
            }
            let sdk_streams: Vec<u32> = events
                .iter()
                .filter_map(dh_devices::detchannel::stream_guest_event_payload)
                .map(|(stream, _payload)| u32::from(stream))
                .collect();
            ReplayExitEvents { sdk_streams }
        }
        kvm_ioctls::VcpuExit::IoIn(port, data)
            if (dh_vmm::kvm::PIO_DETCALL_BASE..detcall_end).contains(&port) =>
        {
            let host = replay_detchannel_mut::<M>(&mut rail.bus).ok_or_else(|| {
                BoundaryError::Exit("detchannel PIO without DetChannelDevice".into())
            })?;
            let value = host.host_mut().pio_in(port, &mut ctx);
            data.fill(0);
            let bytes = value.to_le_bytes();
            let n = data.len().min(4);
            data[..n].copy_from_slice(&bytes[..n]);
            if host.host().metrics.any_anomaly() {
                return Err(BoundaryError::Exit("detchannel drain anomaly".into()));
            }
            ReplayExitEvents {
                sdk_streams: Vec::new(),
            }
        }
        kvm_ioctls::VcpuExit::IoIn(_port, data) => {
            data.fill(0);
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::IoOut(_port, _data) => ReplayExitEvents::default(),
        kvm_ioctls::VcpuExit::MmioRead(gpa, data) => {
            rail.bus
                .read(gpa, data, &mut ctx)
                .map_err(|e| BoundaryError::Exit(format!("bus read {gpa:#x}: {e:?}")))?;
            ReplayExitEvents::default()
        }
        kvm_ioctls::VcpuExit::MmioWrite(gpa, data) => {
            rail.bus
                .write(gpa, data, &mut ctx)
                .map_err(|e| BoundaryError::Exit(format!("bus write {gpa:#x}: {e:?}")))?;
            ReplayExitEvents::default()
        }
        other => {
            return Err(BoundaryError::Exit(format!("unexpected exit: {other:?}")));
        }
    };
    if let Some(e) = ctx.log_fault() {
        return Err(BoundaryError::Exit(format!("log fault: {e:?}")));
    }
    Ok(sdk_streams)
}

fn replay_detchannel_drain_at_pause<M>(
    rail: &mut DeviceRail<M>,
    icount: u64,
) -> Result<(), BoundaryError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
{
    let mut ctx = dh_devices::DevCtx::new(
        icount,
        0,
        &mut rail.log,
        &mut rail.mem,
        &mut rail.entropy,
        &mut rail.irqs,
    );
    let Some(host) = replay_detchannel_mut::<M>(&mut rail.bus) else {
        return Ok(());
    };
    host.host_mut().drain_at_pause(&mut ctx);
    if host.host().metrics.any_anomaly() {
        return Err(BoundaryError::Exit("detchannel pause drain anomaly".into()));
    }
    if let Some(e) = ctx.log_fault() {
        return Err(BoundaryError::Exit(format!(
            "detchannel pause drain log fault: {e:?}"
        )));
    }
    Ok(())
}

fn detchannel_exit_generated_event(device_id: u16, event_type: u16) -> bool {
    device_id == dh_inputlog::dhilog::DEVICE_ID_DETCHANNEL
        && matches!(
            event_type,
            dh_inputlog::dhilog::EVENT_PIO_ANSWER | dh_inputlog::dhilog::EVENT_CONS_BUMP
        )
}

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
    /// Terminal mismatch refined by replay-vs-recorded checkpoint evidence.
    BisectionDivergence(BisectionDivergence),
    /// Bisection was requested, but the log does not contain evidence that
    /// honestly covers the observed divergence.
    BisectionPrecondition(String),
    /// Capturing the replay probe snapshot failed.
    BisectionCapture(crate::snapshot_engine::EngineError),
    /// Expected-vs-actual snapshot comparison failed.
    BisectionCompare(crate::snapshot_compare::SnapshotComparisonError),
    /// A canonical record kind this executor cannot apply yet — loud,
    /// never skipped (a skipped input IS a divergence).
    NotYetWired(&'static str),
    /// The caller stopped consuming progress; abort at a deterministic
    /// progress boundary and let the owner clean up the temporary slot.
    Cancelled(&'static str),
    Apply(String),
    Run(String),
}

pub struct ReplayOutcome {
    pub records_applied: u64,
    pub epoch_hashes_verified: u64,
    pub end_icount: u64,
    pub end_state_hash: [u8; 32],
    /// The resealed log produced by the replay's own rail. On success it is
    /// byte-identical to the input unless the input carried diagnostic-only
    /// BISECTION_CHECKPOINT AUX records, which replay verifies by comparing
    /// the normalized non-checkpoint record stream.
    pub resealed: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
struct ComparableReplayHeader {
    version: u16,
    flags_without_has_aux: u32,
    base_snapshot_id: [u8; 32],
    end_snapshot_id: [u8; 32],
    entropy_seed: [u8; 32],
    machine_config_hash: [u8; 32],
    clock_num: u32,
    clock_den: u32,
    end_icount: u64,
    end_vns: u64,
    end_state_hash: [u8; 32],
    encoder_fingerprint: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct ComparableReplayRecord {
    kind: u8,
    rflags: u8,
    icount: u64,
    boundary_rip: u64,
    payload: Vec<u8>,
}

fn comparable_replay_header(header: &Header) -> ComparableReplayHeader {
    ComparableReplayHeader {
        version: header.version,
        flags_without_has_aux: header.flags & !dh_inputlog::dhilog::FLAG_HAS_AUX,
        base_snapshot_id: header.base_snapshot_id,
        end_snapshot_id: header.end_snapshot_id,
        entropy_seed: header.entropy_seed,
        machine_config_hash: header.machine_config_hash,
        clock_num: header.clock_num,
        clock_den: header.clock_den,
        end_icount: header.end_icount,
        end_vns: header.end_vns,
        end_state_hash: header.end_state_hash,
        encoder_fingerprint: header.encoder_fingerprint,
    }
}

fn has_bisection_checkpoint(log: &LogReader<'_>) -> bool {
    log.records()
        .any(|rec| matches!(rec.body(), RecordBody::BisectionCheckpoint { .. }))
}

fn comparable_replay_records(log: &LogReader<'_>) -> Vec<ComparableReplayRecord> {
    log.records()
        .filter(|rec| !matches!(rec.body(), RecordBody::BisectionCheckpoint { .. }))
        .map(|rec| ComparableReplayRecord {
            kind: rec.kind(),
            rflags: rec.rflags(),
            icount: rec.icount(),
            boundary_rip: rec.boundary_rip(),
            payload: rec.payload().to_vec(),
        })
        .collect()
}

fn reseal_equivalent_ignoring_bisection_checkpoints(
    resealed: &[u8],
    recorded: &LogReader<'_>,
) -> Result<bool, ReplayError> {
    let replayed = LogReader::parse(resealed).map_err(ReplayError::Log)?;
    if !has_bisection_checkpoint(recorded) && !has_bisection_checkpoint(&replayed) {
        return Ok(false);
    }
    Ok(
        comparable_replay_header(replayed.header()) == comparable_replay_header(recorded.header())
            && comparable_replay_records(&replayed) == comparable_replay_records(recorded),
    )
}

#[derive(Clone, Debug)]
struct ActualBisectionProbe {
    checkpoint_position: RecordPosition,
    snapshot_ref: SnapshotRef,
}

#[derive(Clone, Copy, Debug)]
struct ReplayBisectionBase {
    cumulative_icount: u64,
    vns: u64,
    epoch_index: u64,
    segment_start_vns: u64,
}

#[derive(Clone, Copy, Debug)]
struct PendingBisectionDivergence {
    selected: SelectedBisectionCheckpoint,
    what: &'static str,
    expected_hash: [u8; 32],
    got_hash: [u8; 32],
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn select_epoch_bisection(
    bisection_index: Option<&BisectionCheckpointIndex>,
    epoch_index: u64,
    at_icount: u64,
) -> Result<SelectedBisectionCheckpoint, ReplayError> {
    let index = bisection_index.ok_or_else(|| {
        ReplayError::BisectionPrecondition(
            "VerifyReplay divergence bisection requires recorded bisection checkpoints".into(),
        )
    })?;
    index
        .select_for_divergence(BisectionSelectionTarget::EpochHash {
            epoch_index,
            at_icount,
        })
        .ok_or_else(|| {
            ReplayError::BisectionPrecondition(format!(
                "VerifyReplay bisection has no checkpoint evidence covering epoch {epoch_index} at icount {at_icount}"
            ))
        })
}

fn select_terminal_bisection(
    bisection_index: Option<&BisectionCheckpointIndex>,
    end_icount: u64,
) -> Result<SelectedBisectionCheckpoint, ReplayError> {
    let index = bisection_index.ok_or_else(|| {
        ReplayError::BisectionPrecondition(
            "VerifyReplay divergence bisection requires recorded bisection checkpoints".into(),
        )
    })?;
    index
        .select_for_divergence(BisectionSelectionTarget::TerminalEndState { end_icount })
        .ok_or_else(|| {
            ReplayError::BisectionPrecondition(format!(
                "VerifyReplay bisection has no checkpoint evidence covering terminal divergence at icount {end_icount}"
            ))
        })
}

fn selected_checkpoint_is_at_epoch(
    selected: SelectedBisectionCheckpoint,
    epoch_index: u64,
    icount: u64,
) -> bool {
    selected.checkpoint.preceding_epoch_hash.epoch_index == epoch_index
        && selected.checkpoint.preceding_epoch_hash.position.icount == icount
}

fn capture_bisection_probe<M>(
    slot: &SlotVm,
    rail: &DeviceRail<M>,
    machine_config: &MachineConfig,
    store: &SnapstoreClient,
    base: ReplayBisectionBase,
    checkpoint: IndexedBisectionCheckpoint,
    boundary_icount: u64,
    chain_value: [u8; 32],
) -> Result<ActualBisectionProbe, ReplayError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
{
    if boundary_icount != checkpoint.checkpoint_icount {
        return Err(ReplayError::BisectionPrecondition(format!(
            "VerifyReplay bisection probe landed at icount {boundary_icount}, expected checkpoint icount {}",
            checkpoint.checkpoint_icount
        )));
    }
    let vns = machine_config
        .clock
        .vns_from_icount(boundary_icount)
        .ok_or_else(|| {
            ReplayError::BisectionPrecondition(format!(
                "VerifyReplay bisection probe vns conversion overflow at icount {boundary_icount}"
            ))
        })?;
    if vns != checkpoint.checkpoint_vns {
        return Err(ReplayError::BisectionPrecondition(format!(
            "VerifyReplay bisection checkpoint vns mismatch at icount {boundary_icount}: log {}, computed {vns}",
            checkpoint.checkpoint_vns
        )));
    }
    let vns_delta = vns.saturating_sub(base.segment_start_vns);
    let epoch_len = machine_config.epoch_len.max(1);
    let epoch_delta = boundary_icount / epoch_len;
    let boundary = crate::snapshot_engine::BoundaryState {
        icount: base.cumulative_icount.saturating_add(boundary_icount),
        vns: base.vns.saturating_add(vns_delta),
        epoch_index: base.epoch_index.saturating_add(epoch_delta),
        hash_chain: chain_value,
        agenda_empty: false,
    };
    let snapshot = crate::snapshot_engine::capture_bisection_checkpoint_snapshot_with_lapic(
        slot,
        dh_vmm::SlotState::Paused,
        &rail.bus,
        &rail.lapic,
        &rail.entropy,
        machine_config,
        boundary,
        store,
    )
    .map_err(ReplayError::BisectionCapture)?;

    Ok(ActualBisectionProbe {
        checkpoint_position: checkpoint.position,
        snapshot_ref: snapshot.snapshot_ref,
    })
}

fn bisection_divergence_from_probe(
    selected: SelectedBisectionCheckpoint,
    actual_probe_ref: SnapshotRef,
    what: &'static str,
    expected_hash: [u8; 32],
    got_hash: [u8; 32],
    store: &SnapstoreClient,
) -> Result<BisectionDivergence, ReplayError> {
    let expected_ref = SnapshotRef::from_bytes(selected.checkpoint.checkpoint_snapshot_ref);
    let comparison =
        crate::snapshot_compare::compare_snapshots(store, expected_ref, actual_probe_ref.clone())
            .map_err(ReplayError::BisectionCompare)?;
    let first_bad_epoch = match selected.divergence {
        BisectionDivergenceSite::EpochHash { epoch_index, .. } => Some(epoch_index),
        BisectionDivergenceSite::Terminal { .. } => None,
    };

    Ok(BisectionDivergence {
        first_bad_epoch,
        icount_lo: selected.coverage_icount_lo,
        icount_hi: selected.coverage_icount_hi,
        rip_expected: comparison.rip_expected,
        rip_actual: comparison.rip_actual,
        reg_diff: comparison.reg_diff,
        diff_page_idx: comparison.diff_page_idx,
        suspected_cause: format!(
            "replay-vs-recorded:{what}; expected_hash={}; got_hash={}",
            hex32(&expected_hash),
            hex32(&got_hash)
        ),
        evidence: BisectionEvidence {
            mode: BisectionMode::ReplayVsRecorded,
            expected_checkpoint_ref: Some(selected.checkpoint.checkpoint_snapshot_ref),
            actual_probe_ref: Some(actual_probe_ref.to_bytes()),
            coverage_icount_lo: selected.coverage_icount_lo,
            coverage_icount_hi: selected.coverage_icount_hi,
        },
    })
}

/// Replay one sealed segment from its base snapshot. `slot` and `bus`
/// are FRESH (the restore engine's preconditions); the counter is
/// routed to this thread and will be reset by the restore (§3.1).
#[allow(clippy::too_many_arguments)]
pub fn replay_segment<M>(
    slot: &mut SlotVm,
    rail: DeviceRail<M>,
    machine_config: &MachineConfig,
    base_snapshot: SnapshotRef,
    counter: &InstRetired,
    store: &SnapstoreClient,
    log_bytes: &[u8],
) -> Result<ReplayOutcome, ReplayError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
{
    replay_segment_with_epoch_progress(
        slot,
        rail,
        machine_config,
        base_snapshot,
        counter,
        store,
        log_bytes,
        |_epoch_index, _icount, _chain_value| Ok(()),
    )
}

/// Same executor as [`replay_segment`], with a callback after every
/// EPOCH_HASH link has matched and been written to the replay log. The
/// worker RPC uses this to stream `EpochOk` without waiting for END.
#[allow(clippy::too_many_arguments)]
pub fn replay_segment_with_epoch_progress<M, F>(
    slot: &mut SlotVm,
    rail: DeviceRail<M>,
    machine_config: &MachineConfig,
    base_snapshot: SnapshotRef,
    counter: &InstRetired,
    store: &SnapstoreClient,
    log_bytes: &[u8],
    on_epoch_ok: F,
) -> Result<ReplayOutcome, ReplayError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
    F: FnMut(u64, u64, [u8; 32]) -> Result<(), ReplayError>,
{
    replay_segment_with_epoch_progress_and_bisection(
        slot,
        rail,
        machine_config,
        base_snapshot,
        counter,
        store,
        log_bytes,
        None,
        on_epoch_ok,
    )
}

/// Bisection-aware replay. When `bisection_index` is present, epoch and
/// terminal hash divergences are refined using recorded checkpoint snapshots
/// that honestly cover the observed site.
#[allow(clippy::too_many_arguments)]
pub fn replay_segment_with_epoch_progress_and_bisection<M, F>(
    slot: &mut SlotVm,
    rail: DeviceRail<M>,
    machine_config: &MachineConfig,
    base_snapshot: SnapshotRef,
    counter: &InstRetired,
    store: &SnapstoreClient,
    log_bytes: &[u8],
    bisection_index: Option<&BisectionCheckpointIndex>,
    on_epoch_ok: F,
) -> Result<ReplayOutcome, ReplayError>
where
    M: GuestMem + detguest_host::GuestMem + Clone + Send + 'static,
    F: FnMut(u64, u64, [u8; 32]) -> Result<(), ReplayError>,
{
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
    rail.lapic = restored.lapic;
    let segment_start_vns = machine_config
        .clock
        .vns_from_icount(0)
        .ok_or_else(|| ReplayError::Run("segment-start vns conversion overflow".into()))?;
    let bisection_base = ReplayBisectionBase {
        cumulative_icount: restored.cumulative_icount,
        vns: restored.vns,
        epoch_index: restored.epoch_index,
        segment_start_vns,
    };
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
    let terminal_sdk_streams: Vec<u32> = log
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::SdkEvent { stream, .. } if rec.icount() == header.end_icount => {
                Some(u32::from(stream))
            }
            _ => None,
        })
        .collect();
    let terminal_sdk_streams = terminal_sdk_streams
        .last()
        .copied()
        .map(|stream| vec![stream])
        .unwrap_or_default();
    let terminal_selection = bisection_index.and_then(|index| {
        index.select_for_divergence(BisectionSelectionTarget::TerminalEndState {
            end_icount: header.end_icount,
        })
    });
    let terminal_probe = std::cell::RefCell::new(None::<ActualBisectionProbe>);
    let mut epoch_after_canonical_icounts = Vec::new();
    let mut current_icount = None;
    let mut saw_canonical_at_icount = false;
    for rec in log.records() {
        if current_icount != Some(rec.icount()) {
            current_icount = Some(rec.icount());
            saw_canonical_at_icount = false;
        }
        match rec.body() {
            RecordBody::PadSet { .. } | RecordBody::NetRx { .. } | RecordBody::DevEvent { .. } => {
                saw_canonical_at_icount = true;
            }
            RecordBody::EpochHash { .. } if saw_canonical_at_icount => {
                if !epoch_after_canonical_icounts.contains(&rec.icount()) {
                    epoch_after_canonical_icounts.push(rec.icount());
                }
            }
            RecordBody::End { .. } => break,
            _ => {}
        }
    }
    let on_epoch_ok = std::cell::RefCell::new(on_epoch_ok);

    let pause = AtomicBool::new(false);
    let mut records_applied = 0u64;

    // One run quantum to `target` (absolute), servicing exits through the
    // rail. Each epoch link is verified against the recording AT THE
    // LINK POINT (the sink) and re-landed in the replay's own log; a
    // mismatch aborts the quantum loudly through the sink error.
    let verified = std::cell::Cell::new(0usize);
    let last_epoch_icount = std::cell::Cell::new(None);
    let divergence: DivergenceCell = std::cell::Cell::new(None);
    let bisection_error = std::cell::RefCell::new(None);
    let pending_bisection = std::cell::RefCell::new(None::<PendingBisectionDivergence>);
    let progress_error = std::cell::RefCell::new(None);
    let stopped_sdk_streams = std::cell::RefCell::new(Vec::<u32>::new());
    let run_to = |slot: &mut SlotVm,
                  chain: &mut StateHashChain,
                  target: u64,
                  hash_final_stop: bool,
                  hash_final_epoch: bool,
                  sdk_streams: Option<&[u32]>|
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
        let sdk_streams = sdk_streams.filter(|streams| !streams.is_empty());
        let event_stop = sdk_streams.is_some();
        stopped_sdk_streams.borrow_mut().clear();
        if target == start && !event_stop {
            return Ok(None);
        }
        let sdk_event_feed = std::cell::Cell::new(0u64);
        let out = {
            let hash_device_sections = || {
                let rail_ref = rail.borrow();
                runtime_hash_device_sections(&rail_ref.bus, &rail_ref.lapic)
            };
            let mut seg = Segment {
                slot,
                counter,
                chain,
                config: machine_config,
                start_icount: start,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: event_stop.then_some(&sdk_event_feed),
                hash_device_sections: Some(&hash_device_sections),
            };
            let until = if event_stop {
                let hard_cap = target
                    .checked_sub(start)
                    .and_then(|budget| budget.checked_add(1_000_000))
                    .ok_or_else(|| ReplayError::Apply("NextSdkEvent hard cap overflows".into()))?;
                Until::NextSdkEvent { hard_cap }
            } else {
                Until::IcountBudget(target - start)
            };
            run_segment_with_epoch_options(
                &mut seg,
                until,
                RunOptions {
                    hash_final_stop,
                    hash_final_epoch,
                    ..RunOptions::default()
                },
                &mut || false,
                &mut |exit| {
                    let icount = counter
                        .read()
                        .map_err(|e| BoundaryError::Exit(format!("counter: {e:?}")))?;
                    let exit_events = replay_service_exit(&mut rail.borrow_mut(), icount, exit)?;
                    if let Some(want) = sdk_streams {
                        if exit_events
                            .sdk_streams
                            .iter()
                            .any(|stream| want.contains(stream))
                        {
                            stopped_sdk_streams.replace(exit_events.sdk_streams.clone());
                            sdk_event_feed.set(sdk_event_feed.get() + 1);
                        }
                    }
                    Ok(())
                },
                &mut |idx, boundary, value, epoch_slot| {
                    let icount = boundary.icount;
                    if let Some(pending) = *pending_bisection.borrow() {
                        if selected_checkpoint_is_at_epoch(pending.selected, idx, icount) {
                            let Some(epoch_slot) = epoch_slot else {
                                bisection_error.replace(Some(
                                    ReplayError::BisectionPrecondition(format!(
                                        "VerifyReplay bisection checkpoint at icount {icount} is not checkpoint-safe"
                                    )),
                                ));
                                return Err(BoundaryError::Exit(
                                    "bisection epoch divergence".into(),
                                ));
                            };
                            let outcome = capture_bisection_probe(
                                epoch_slot,
                                &rail.borrow(),
                                machine_config,
                                store,
                                bisection_base,
                                pending.selected.checkpoint,
                                icount,
                                value,
                            )
                            .and_then(|probe| {
                                bisection_divergence_from_probe(
                                    pending.selected,
                                    probe.snapshot_ref,
                                    pending.what,
                                    pending.expected_hash,
                                    pending.got_hash,
                                    store,
                                )
                            });
                            bisection_error.replace(Some(match outcome {
                                Ok(divergence) => ReplayError::BisectionDivergence(divergence),
                                Err(e) => e,
                            }));
                            return Err(BoundaryError::Exit(
                                "bisection epoch divergence".into(),
                            ));
                        }
                        rail.borrow_mut()
                            .log_epoch_hash(idx, icount, value)
                            .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))?;
                        return Ok(());
                    }
                    let i = verified.get();
                    match expected_epochs.get(i) {
                        Some((e_idx, e_icount, e_value))
                            if *e_idx == idx && *e_icount == icount && *e_value == value => {}
                        Some((e_idx, e_icount, e_value)) => {
                            if bisection_index.is_some() {
                                let selected = if *e_icount == icount {
                                    match select_epoch_bisection(bisection_index, *e_idx, icount) {
                                        Ok(selected) => selected,
                                        Err(e) => {
                                            bisection_error.replace(Some(e));
                                            return Err(BoundaryError::Exit(
                                                "bisection epoch divergence".into(),
                                            ));
                                        }
                                    }
                                } else {
                                    bisection_error.replace(Some(
                                        ReplayError::BisectionPrecondition(format!(
                                            "VerifyReplay bisection cannot select checkpoint evidence: expected epoch {e_idx} at icount {e_icount}, replay reached icount {icount}"
                                        )),
                                    ));
                                    return Err(BoundaryError::Exit(
                                        "bisection epoch divergence".into(),
                                    ));
                                };
                                let pending = PendingBisectionDivergence {
                                    selected,
                                    what: "EPOCH_HASH chain value",
                                    expected_hash: *e_value,
                                    got_hash: value,
                                };
                                let outcome =
                                    if selected_checkpoint_is_at_epoch(selected, idx, icount) {
                                    let Some(epoch_slot) = epoch_slot else {
                                        bisection_error.replace(Some(
                                            ReplayError::BisectionPrecondition(format!(
                                                "VerifyReplay bisection checkpoint at icount {icount} is not checkpoint-safe"
                                            )),
                                        ));
                                        return Err(BoundaryError::Exit(
                                            "bisection epoch divergence".into(),
                                        ));
                                    };
                                    match capture_bisection_probe(
                                        epoch_slot,
                                        &rail.borrow(),
                                        machine_config,
                                        store,
                                        bisection_base,
                                        selected.checkpoint,
                                        icount,
                                        value,
                                    )
                                    .and_then(|probe| {
                                        bisection_divergence_from_probe(
                                            selected,
                                            probe.snapshot_ref,
                                            pending.what,
                                            pending.expected_hash,
                                            pending.got_hash,
                                            store,
                                        )
                                    }) {
                                        Ok(divergence) => {
                                            ReplayError::BisectionDivergence(divergence)
                                        }
                                        Err(e) => e,
                                    }
                                } else {
                                    pending_bisection.replace(Some(pending));
                                    rail.borrow_mut()
                                        .log_epoch_hash(idx, icount, value)
                                        .map_err(|e| {
                                            BoundaryError::Exit(format!("epoch log: {e:?}"))
                                        })?;
                                    return Ok(());
                                };
                                bisection_error.replace(Some(outcome));
                                return Err(BoundaryError::Exit(
                                    "bisection epoch divergence".into(),
                                ));
                            }
                            divergence.set(Some((
                                "EPOCH_HASH chain value",
                                icount,
                                *e_value,
                                value,
                            )));
                            return Err(BoundaryError::Exit("epoch divergence".into()));
                        }
                        None => {
                            if bisection_index.is_some() {
                                bisection_error.replace(Some(ReplayError::BisectionPrecondition(
                                    format!(
                                        "VerifyReplay bisection cannot select checkpoint evidence: replay produced extra epoch {idx} at icount {icount}"
                                    ),
                                )));
                                return Err(BoundaryError::Exit(
                                    "bisection epoch divergence".into(),
                                ));
                            }
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
                    last_epoch_icount.set(Some(icount));
                    if let Some(selected) = terminal_selection {
                        if selected_checkpoint_is_at_epoch(selected, idx, icount) {
                            let Some(epoch_slot) = epoch_slot else {
                                bisection_error.replace(Some(
                                    ReplayError::BisectionPrecondition(format!(
                                        "VerifyReplay bisection checkpoint at icount {icount} is not checkpoint-safe"
                                    )),
                                ));
                                return Err(BoundaryError::Exit(
                                    "terminal bisection probe capture failed".into(),
                                ));
                            };
                            match capture_bisection_probe(
                                epoch_slot,
                                &rail.borrow(),
                                machine_config,
                                store,
                                bisection_base,
                                selected.checkpoint,
                                icount,
                                value,
                            ) {
                                Ok(probe) => {
                                    terminal_probe.replace(Some(probe));
                                }
                                Err(e) => {
                                    bisection_error.replace(Some(e));
                                    return Err(BoundaryError::Exit(
                                        "terminal bisection probe capture failed".into(),
                                    ));
                                }
                            }
                        }
                    }
                    rail.borrow_mut()
                        .log_epoch_hash(idx, icount, value)
                        .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))?;
                    if let Err(e) = on_epoch_ok.borrow_mut()(idx, icount, value) {
                        progress_error.replace(Some(e));
                        return Err(BoundaryError::Exit("replay progress stopped".into()));
                    }
                    Ok(())
                },
            )
            .map_err(|e: RunError| {
                // Structured side channel (iteration-88 review I1): the
                // sink can only surface a BoundaryError; the real
                // diagnostics travel through the Cell.
                if let Some(e) = progress_error.borrow_mut().take() {
                    e
                } else if let Some(e) = bisection_error.borrow_mut().take() {
                    e
                } else if let Some((what, at_icount, expected, got)) = divergence.take() {
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
    macro_rules! verify_current_epoch {
        ($slot:expr, $chain:expr, $icount:expr) => {{
            'verify_current_epoch: {
            let i = verified.get();
            match expected_epochs.get(i) {
                Some((_, e_icount, _)) if *e_icount != $icount => Ok(false),
                Some((e_idx, e_icount, e_value)) => {
                    let vns = machine_config
                        .clock
                        .vns_from_icount($icount)
                        .ok_or_else(|| ReplayError::Run("vns/icount conversion overflow".into()))?;
                    let device_sections = {
                        let rail_ref = rail.borrow();
                        runtime_hash_device_sections(&rail_ref.bus, &rail_ref.lapic)
                    };
                    $chain
                        .push_final_link($slot, &device_sections, $icount, vns)
                        .map_err(|e| ReplayError::Run(format!("{e:?}")))?;
                    let epoch = machine_config.epoch_len.max(1);
                    let idx = $icount / epoch;
                    let value = $chain.value();
                    if let Some(pending) = *pending_bisection.borrow() {
                        if selected_checkpoint_is_at_epoch(pending.selected, idx, $icount) {
                            let probe = capture_bisection_probe(
                                $slot,
                                &rail.borrow(),
                                machine_config,
                                store,
                                bisection_base,
                                pending.selected.checkpoint,
                                $icount,
                                value,
                            )?;
                            let divergence = bisection_divergence_from_probe(
                                pending.selected,
                                probe.snapshot_ref,
                                pending.what,
                                pending.expected_hash,
                                pending.got_hash,
                                store,
                            )?;
                            return Err(ReplayError::BisectionDivergence(divergence));
                        }
                        rail.borrow_mut()
                            .log_epoch_hash(idx, $icount, value)
                            .map_err(|e| ReplayError::Apply(format!("epoch log: {e:?}")))?;
                        break 'verify_current_epoch Ok(true);
                    }
                    if *e_idx != idx || *e_icount != $icount || *e_value != value {
                        if bisection_index.is_some() {
                            let selected = if *e_icount == $icount {
                                select_epoch_bisection(bisection_index, *e_idx, $icount)?
                            } else {
                                return Err(ReplayError::BisectionPrecondition(format!(
                                    "VerifyReplay bisection cannot select checkpoint evidence: expected epoch {e_idx} at icount {e_icount}, replay reached icount {}",
                                    $icount
                                )));
                            };
                            let pending = PendingBisectionDivergence {
                                selected,
                                what: "EPOCH_HASH chain value",
                                expected_hash: *e_value,
                                got_hash: value,
                            };
                            if selected_checkpoint_is_at_epoch(selected, idx, $icount) {
                                let probe = capture_bisection_probe(
                                    $slot,
                                    &rail.borrow(),
                                    machine_config,
                                    store,
                                    bisection_base,
                                    selected.checkpoint,
                                    $icount,
                                    value,
                                )?;
                                let divergence = bisection_divergence_from_probe(
                                    selected,
                                    probe.snapshot_ref,
                                    pending.what,
                                    pending.expected_hash,
                                    pending.got_hash,
                                    store,
                                )?;
                                return Err(ReplayError::BisectionDivergence(divergence));
                            }
                            pending_bisection.replace(Some(pending));
                            rail.borrow_mut()
                                .log_epoch_hash(idx, $icount, value)
                                .map_err(|e| ReplayError::Apply(format!("epoch log: {e:?}")))?;
                            break 'verify_current_epoch Ok(true);
                        }
                        return Err(ReplayError::Divergence {
                            what: "EPOCH_HASH chain value",
                            at_icount: $icount,
                            expected: *e_value,
                            got: value,
                        });
                    }
                    verified.set(i + 1);
                    last_epoch_icount.set(Some($icount));
                    if let Some(selected) = terminal_selection {
                        if selected_checkpoint_is_at_epoch(selected, idx, $icount) {
                            let probe = capture_bisection_probe(
                                $slot,
                                &rail.borrow(),
                                machine_config,
                                store,
                                bisection_base,
                                selected.checkpoint,
                                $icount,
                                value,
                            )?;
                            terminal_probe.replace(Some(probe));
                        }
                    }
                    rail.borrow_mut()
                        .log_epoch_hash(idx, $icount, value)
                        .map_err(|e| ReplayError::Apply(format!("epoch log: {e:?}")))?;
                    on_epoch_ok.borrow_mut()(idx, $icount, value)?;
                    Ok(true)
                }
                None => Ok(false),
            }
            }
        }};
    }
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
    let terminal_bisection_divergence = |what: &'static str,
                                         expected_hash: [u8; 32],
                                         got_hash: [u8; 32]|
     -> Result<BisectionDivergence, ReplayError> {
        let selected = select_terminal_bisection(bisection_index, header.end_icount)?;
        let actual_probe_ref = {
            let probe = terminal_probe.borrow();
            let probe = probe
                .as_ref()
                .filter(|probe| probe.checkpoint_position == selected.checkpoint.position)
                .ok_or_else(|| {
                    ReplayError::BisectionPrecondition(format!(
                        "VerifyReplay bisection terminal probe for checkpoint seq {} at icount {} was not captured",
                        selected.checkpoint.position.seq,
                        selected.checkpoint.checkpoint_icount
                    ))
                })?;
            probe.snapshot_ref.clone()
        };
        bisection_divergence_from_probe(
            selected,
            actual_probe_ref,
            what,
            expected_hash,
            got_hash,
            store,
        )
    };

    // ── Walk the canonical records ────────────────────────────────────────
    let canonical: Vec<_> = log.canonical().collect();
    let mut last_canonical_icount = None;
    for (index, rec) in canonical.iter().copied().enumerate() {
        let icount = rec.icount();
        let rip = rec.boundary_rip();
        let epoch_after_canonical = epoch_after_canonical_icounts.contains(&icount);
        match rec.body() {
            RecordBody::End { .. } => break, // handled after the loop
            RecordBody::PadSet {
                port,
                buttons,
                frame_hint,
            } => {
                let o = run_to(
                    slot,
                    &mut chain,
                    icount,
                    false,
                    !epoch_after_canonical,
                    None,
                )?;
                require_landed(&o, icount)?;
                rail.borrow_mut()
                    .apply_pad_set(icount, rip, port, buttons, frame_hint)
                    .map_err(|e: RecordError| ReplayError::Apply(format!("{e:?}")))?;
            }
            RecordBody::NetRx { frame } => {
                let o = run_to(
                    slot,
                    &mut chain,
                    icount,
                    false,
                    !epoch_after_canonical,
                    None,
                )?;
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
                let detchannel_will_regenerate =
                    detchannel_exit_generated_event(device_id, event_type)
                        && replay_detchannel_mut::<M>(&mut rail.borrow_mut().bus).is_some();
                if detchannel_will_regenerate {
                    continue;
                }
                let o = run_to(
                    slot,
                    &mut chain,
                    icount,
                    false,
                    !epoch_after_canonical,
                    None,
                )?;
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
        last_canonical_icount = Some(icount);
        if canonical
            .get(index + 1)
            .map_or(true, |next| next.icount() != icount)
            && epoch_after_canonical
        {
            let _ = verify_current_epoch!(slot, &mut chain, icount)?;
        }
        records_applied += 1;
    }

    // ── Run out the tail and check the END identity ───────────────────────
    let (stop_reason_byte, _) = log.end();
    let expected_reason = stop_reason_from_u8(stop_reason_byte)?;
    let verified_before_tail = verified.get();
    let chain_before_tail = chain.clone();
    let terminal_sdk_stream = terminal_sdk_streams.first().copied();
    let tail = if terminal_sdk_stream.is_some() {
        loop {
            let event_tail = run_to(
                slot,
                &mut chain,
                header.end_icount,
                true,
                true,
                Some(&terminal_sdk_streams),
            )?;
            let Some(out) = event_tail else {
                return Err(ReplayError::Run(format!(
                    "tail did not observe terminal SDK event before recording end at {}",
                    header.end_icount
                )));
            };
            if out.reason != StopReason::NextSdkEvent || out.boundary.icount != header.end_icount {
                return Err(ReplayError::Run(format!(
                    "tail stopped {:?} at {} while waiting for terminal SDK event (recording ended {:?} at {})",
                    out.reason, out.boundary.icount, expected_reason, header.end_icount
                )));
            }
            if out.vns != header.end_vns {
                if bisection_index.is_some() {
                    let divergence = terminal_bisection_divergence(
                        "end_vns",
                        u64_hash(header.end_vns),
                        u64_hash(out.vns),
                    )?;
                    return Err(ReplayError::BisectionDivergence(divergence));
                }
                return Err(ReplayError::Divergence {
                    what: "end_vns",
                    at_icount: header.end_icount,
                    expected: u64_hash(header.end_vns),
                    got: u64_hash(out.vns),
                });
            }
            let streams = stopped_sdk_streams.borrow().clone();
            if terminal_sdk_streams
                .iter()
                .any(|want| streams.contains(want))
            {
                replay_detchannel_drain_at_pause(&mut rail.borrow_mut(), header.end_icount)
                    .map_err(|e| ReplayError::Run(format!("{e:?}")))?;
                break Some(out);
            }
            if out.boundary.icount == header.end_icount {
                return Err(ReplayError::Run(format!(
                    "tail reached recording end at {} without terminal SDK stream {:?}",
                    header.end_icount, terminal_sdk_stream
                )));
            }
        }
    } else {
        let tail = run_to(slot, &mut chain, header.end_icount, true, true, None)?;
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
                if bisection_index.is_some() {
                    let divergence = terminal_bisection_divergence(
                        "end_vns",
                        u64_hash(header.end_vns),
                        u64_hash(out.vns),
                    )?;
                    return Err(ReplayError::BisectionDivergence(divergence));
                }
                return Err(ReplayError::Divergence {
                    what: "end_vns",
                    at_icount: header.end_icount,
                    expected: u64_hash(header.end_vns),
                    got: u64_hash(out.vns),
                });
            }
        }
        tail
    };
    if terminal_sdk_stream.is_none()
        && tail.is_none()
        && last_canonical_icount == Some(header.end_icount)
        && last_epoch_icount.get() != Some(header.end_icount)
    {
        let device_sections = {
            let rail_ref = rail.borrow();
            runtime_hash_device_sections(&rail_ref.bus, &rail_ref.lapic)
        };
        chain
            .push_final_link(slot, &device_sections, header.end_icount, header.end_vns)
            .map_err(|e| ReplayError::Run(format!("{e:?}")))?;
    }
    let mut live_end = chain.value();
    if live_end != header.end_state_hash
        && terminal_sdk_stream.is_none()
        && expected_reason == StopReason::BudgetReached
        && matches!(
            tail,
            Some(out)
                if out.reason == StopReason::BudgetReached
                    && out.boundary.icount == header.end_icount
        )
        && verified.get() == verified_before_tail
    {
        // TakeSnapshot seals the active segment as BudgetReached even when
        // the preceding Run stopped on a terminal HLT. An exact replay tail
        // can therefore land just before the HLT and hash the pre-HLT RIP.
        // If no epoch links were emitted, restore the pre-tail chain and
        // allow one more retired-instruction budget so the same-icount HLT
        // exit can produce the recorded hash.
        chain = chain_before_tail;
        let halt_target = header
            .end_icount
            .checked_add(1)
            .ok_or_else(|| ReplayError::Apply("BudgetReached HLT retry overflows".into()))?;
        let halt_tail = run_to(slot, &mut chain, halt_target, true, true, None)?;
        if let Some(out) = &halt_tail {
            if out.reason == StopReason::GuestHalted && out.boundary.icount == header.end_icount {
                if out.vns != header.end_vns {
                    if bisection_index.is_some() {
                        let divergence = terminal_bisection_divergence(
                            "end_vns",
                            u64_hash(header.end_vns),
                            u64_hash(out.vns),
                        )?;
                        return Err(ReplayError::BisectionDivergence(divergence));
                    }
                    return Err(ReplayError::Divergence {
                        what: "end_vns",
                        at_icount: header.end_icount,
                        expected: u64_hash(header.end_vns),
                        got: u64_hash(out.vns),
                    });
                }
                live_end = chain.value();
            }
        }
    }
    let verified = verified.get();
    if verified != expected_epochs.len() {
        if bisection_index.is_some() {
            let divergence = terminal_bisection_divergence(
                "EPOCH_HASH count (recording has more than replay produced)",
                [0; 32],
                [0; 32],
            )?;
            return Err(ReplayError::BisectionDivergence(divergence));
        }
        return Err(ReplayError::Divergence {
            what: "EPOCH_HASH count (recording has more than replay produced)",
            at_icount: header.end_icount,
            expected: [0; 32],
            got: [0; 32],
        });
    }
    if live_end != header.end_state_hash {
        if bisection_index.is_some() {
            let divergence =
                terminal_bisection_divergence("end_state_hash", header.end_state_hash, live_end)?;
            return Err(ReplayError::BisectionDivergence(divergence));
        }
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
    if resealed != log_bytes && !reseal_equivalent_ignoring_bisection_checkpoints(&resealed, &log)?
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use dh_inputlog::dhilog::{LogWriter, SealParams, SegmentHeader};

    fn header() -> SegmentHeader {
        SegmentHeader {
            base_snapshot_id: [0x11; 32],
            entropy_seed: [0x22; 32],
            machine_config_hash: [0x33; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }
    }

    fn seal_params() -> SealParams {
        SealParams {
            end_snapshot_id: [0x44; 32],
            end_icount: 20,
            end_vns: 20,
            end_state_hash: [0x55; 32],
            stop_reason: 1,
        }
    }

    fn log_with_optional_checkpoint(buttons: u32, checkpoint: bool) -> Vec<u8> {
        let mut writer = LogWriter::new(header());
        writer.pad_set(10, 0x1010, 0, buttons, 0).unwrap();
        writer.epoch_hash(20, 0x2020, 2, [0x66; 32]).unwrap();
        if checkpoint {
            writer
                .bisection_checkpoint(20, 0x2020, 20, [0x77; 32], 20)
                .unwrap();
        }
        writer.seal(seal_params()).unwrap()
    }

    #[test]
    fn reseal_comparison_ignores_only_bisection_checkpoint_aux_records() {
        let resealed = log_with_optional_checkpoint(0xA5A5, false);
        let recorded = log_with_optional_checkpoint(0xA5A5, true);
        let recorded_reader = LogReader::parse(&recorded).unwrap();
        assert!(
            reseal_equivalent_ignoring_bisection_checkpoints(&resealed, &recorded_reader).unwrap()
        );

        let changed_canonical = log_with_optional_checkpoint(0x5A5A, false);
        assert!(!reseal_equivalent_ignoring_bisection_checkpoints(
            &changed_canonical,
            &recorded_reader
        )
        .unwrap());

        let plain_reader = LogReader::parse(&resealed).unwrap();
        assert!(
            !reseal_equivalent_ignoring_bisection_checkpoints(&resealed, &plain_reader).unwrap()
        );
    }
}
