//! Tier-A CoW fork orchestration (bead 9e4; ARCH §8.4, the hot path):
//! freeze the parent, give the child a PRIVATE CoW mapping of the
//! parent's memfd plus fresh per-slot KVM fds (`KvmSystem::fork_slot_vm`),
//! and stuff vCPU + device state through the §8.3 codec from the parent's
//! IN-MEMORY DHSNAP — no store round-trip, no page copies.
//!
//! ONE CODEC, TWO TRANSPORTS: the parent's state is assembled by the
//! capture engine's `build_dhsnap` and applied by the restore engine's
//! `apply_dhsnap` — byte-for-byte the same encoding a tier-B restore
//! consumes. Fork transparency therefore reduces to the already-proven
//! snapshot transparency plus the kernel's CoW semantics; there is no
//! fork-only serialization to drift.
//!
//! Preconditions mirror TakeSnapshot's §8.1 set, with one swap: the
//! parent must be FROZEN, not merely Paused — `fork_slot_vm` enforces the
//! kernel half (F_SEAL_FUTURE_WRITE on the memfd) and this engine
//! enforces the state-machine half (`SlotState::Frozen`, the R9 software
//! guard: a frozen parent cannot run while children share its pages).
//! The caller attests the boundary (agenda empty, position) exactly as
//! for a snapshot.
//!
//! What the child does NOT inherit: dirty-tracking state (fresh ring on
//! fresh fds; the dirty set starts empty — the child's first incremental
//! parent is the FORK POINT, recorded by run control), the DHILOG (the
//! caller opens a fresh segment with `base_snapshot_id` = the fork
//! point's ref, §8.4), and the counter axis (pass `counter` to re-zero —
//! the same §3.1 latch as a restore). By default, entropy continues from
//! the fork-point ENTR state; an explicit child segment seed starts a fresh
//! deterministic PRNG stream after the snapshot-equivalent fork is built.

use dh_detclock::counter::InstRetired;
use dh_devices::entropy::DetEntropy;
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::SlotState;

use crate::restore_engine::{apply_dhsnap, RestoreError};
use crate::snapshot_engine::{build_dhsnap, BoundaryState, EngineError};

#[derive(Debug)]
pub enum ForkError {
    /// §8.1: forks only happen at quiescent boundaries.
    AgendaNotEmpty,
    /// The R9 software guard: the parent must be Frozen before any child
    /// shares its pages (the kernel seal alone does not stop the
    /// parent's vCPU from running).
    ParentNotFrozen { state: SlotState },
    /// Assembling the parent's in-memory DHSNAP failed.
    Capture(String),
    /// Building the child slot (CoW mapping, fresh KVM fds) failed —
    /// includes the unsealed-parent kernel check.
    Kvm(String),
    /// Building the child device bus failed after the child mapping exists.
    BuildBus(String),
    /// Stuffing the child from the DHSNAP failed; the child is scrap.
    Apply(String),
}

/// What run control needs to start the child's segment. Same shape as a
/// restore outcome, plus the child slot itself.
pub struct ForkOutcome {
    /// The child slot, Paused at the parent's boundary BY CONVENTION —
    /// the engine returns raw KVM objects; registering the child in the
    /// slot table as `SlotState::Paused` (and accounting it against the
    /// parent's `Frozen{children}` count, R9) is the slot manager's job
    /// (bead ol1). Note `child.ram_is_cow == true`: it cannot be frozen
    /// or re-forked directly — a diverged child becomes a fork base via
    /// TakeSnapshot + restore into a fresh slot.
    pub child: SlotVm,
    pub cumulative_icount: u64,
    pub vns: u64,
    pub epoch_index: u64,
    /// The child's chain resumes from the fork point (`from_value`).
    pub chain: StateHashChain,
    /// The child's PRNG. Without an explicit segment seed it is the
    /// parent's stream position, continued. That identity is CORRECT (§5):
    /// divergence between siblings comes from inputs or from a caller-chosen
    /// new segment seed, never from host entropy or the fork operation itself.
    pub entropy: DetEntropy,
}

/// One tier-A fork, end to end. On success the child is Paused at the
/// parent's boundary with CoW RAM and fresh KVM state; the parent is
/// untouched (and still Frozen — unfreezing while children exist is the
/// slot manager's bookkeeping, risk R9). On error any partially-built
/// child is dropped here; the parent is never modified by this call.
#[allow(clippy::too_many_arguments)]
pub fn fork_slot(
    sys: &KvmSystem,
    parent: &SlotVm,
    parent_state: SlotState,
    parent_bus: &dh_devices::MmioBus,
    parent_entropy: &DetEntropy,
    machine_config: &dh_vmm::config::MachineConfig,
    boundary: BoundaryState,
    entropy_seed: Option<[u8; 32]>,
    child_bus: &mut dh_devices::MmioBus,
    counter: Option<&InstRetired>,
) -> Result<ForkOutcome, ForkError> {
    let dhsnap = prepare_parent_dhsnap(
        parent,
        parent_state,
        parent_bus,
        parent_entropy,
        machine_config,
        boundary,
    )?;
    let child = sys
        .fork_slot_vm(parent)
        .map_err(|e| ForkError::Kvm(format!("{e:?}")))?;
    finish_fork(
        child,
        child_bus,
        machine_config,
        &dhsnap,
        entropy_seed,
        counter,
    )
}

/// Variant for callers whose child bus needs the freshly-created child
/// mapping (DetChannelDevice's guest-memory handle is the concrete case).
#[allow(clippy::too_many_arguments)]
pub fn fork_slot_with_child_bus<F>(
    sys: &KvmSystem,
    parent: &SlotVm,
    parent_state: SlotState,
    parent_bus: &dh_devices::MmioBus,
    parent_entropy: &DetEntropy,
    machine_config: &dh_vmm::config::MachineConfig,
    boundary: BoundaryState,
    entropy_seed: Option<[u8; 32]>,
    counter: Option<&InstRetired>,
    build_child_bus: F,
) -> Result<(ForkOutcome, dh_devices::MmioBus), ForkError>
where
    F: FnOnce(&SlotVm) -> Result<dh_devices::MmioBus, String>,
{
    let dhsnap = prepare_parent_dhsnap(
        parent,
        parent_state,
        parent_bus,
        parent_entropy,
        machine_config,
        boundary,
    )?;
    let child = sys
        .fork_slot_vm(parent)
        .map_err(|e| ForkError::Kvm(format!("{e:?}")))?;
    let mut child_bus = build_child_bus(&child).map_err(ForkError::BuildBus)?;
    let outcome = finish_fork(
        child,
        &mut child_bus,
        machine_config,
        &dhsnap,
        entropy_seed,
        counter,
    )?;
    Ok((outcome, child_bus))
}

#[allow(clippy::too_many_arguments)]
fn prepare_parent_dhsnap(
    parent: &SlotVm,
    parent_state: SlotState,
    parent_bus: &dh_devices::MmioBus,
    parent_entropy: &DetEntropy,
    machine_config: &dh_vmm::config::MachineConfig,
    boundary: BoundaryState,
) -> Result<Vec<u8>, ForkError> {
    if !boundary.agenda_empty {
        return Err(ForkError::AgendaNotEmpty);
    }
    if parent_state != SlotState::Frozen {
        return Err(ForkError::ParentNotFrozen {
            state: parent_state,
        });
    }

    // ── 1. The parent's in-memory DHSNAP (§8.4: "decode the parent's
    //       in-memory DHSNAP, cheap, ~tens of KiB") ───────────────────────
    build_dhsnap(
        parent,
        parent_bus,
        parent_entropy,
        machine_config,
        &boundary,
    )
    .map_err(|e| match e {
        EngineError::Codec(m) => ForkError::Capture(m),
        other => ForkError::Capture(format!("{other:?}")),
    })
}

fn finish_fork(
    child: SlotVm,
    child_bus: &mut dh_devices::MmioBus,
    machine_config: &dh_vmm::config::MachineConfig,
    dhsnap: &[u8],
    entropy_seed: Option<[u8; 32]>,
    counter: Option<&InstRetired>,
) -> Result<ForkOutcome, ForkError> {
    // ── 3. Stuff the child through the one true codec (§8.3 order: RAM
    //       is already live via CoW, then devices, then vCPU) ─────────────
    let applied = apply_dhsnap(&child, child_bus, machine_config, &dhsnap, counter, None).map_err(
        |e| match e {
            RestoreError::Kvm(m) => ForkError::Kvm(m),
            other => ForkError::Apply(format!("{other:?}")),
        },
    )?;

    let entropy = entropy_seed
        .filter(|seed| *seed != [0u8; 32])
        .map(DetEntropy::from_seed)
        .unwrap_or(applied.entropy);

    Ok(ForkOutcome {
        child,
        cumulative_icount: applied.cumulative_icount,
        vns: applied.vns,
        epoch_index: applied.epoch_index,
        chain: applied.chain,
        entropy,
    })
}
