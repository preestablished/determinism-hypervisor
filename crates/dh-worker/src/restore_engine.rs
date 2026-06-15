//! RestoreSnapshot orchestration, tier B (bead 9wa; ARCH §8.3): fetch the
//! manifest + flattened page set from the REAL snapshot-store, materialize
//! guest RAM, then restore in the §8.3 order — **RAM first, then devices,
//! then vCPU** — and re-seed the segment clocks (PvClock `vns_base`, chain
//! `from_value`, counter re-zero).
//!
//! ORDER IS LOAD-BEARING: §8.3 fixes RAM → devices → vCPU because device
//! restore is allowed to validate against live guest RAM. No bus device
//! exercises that yet — DetChannelHost's EVTC re-attach (detchannel.rs) is
//! the intended consumer, but it does not implement `DetDevice` today (its
//! restore needs a fault plan; putting it on the bus is slot-manager
//! integration, bead ol1) — the engine honors the contract NOW so that
//! landing never reorders this function.
//! The vCPU goes last; `vcpu_state::restore` owns the §8.3 KVM_SET_* order
//! internally (SREGS→REGS→FPU→XCRS→XSAVE→DEBUGREGS→EVENTS→MSRs last,
//! IA32_TSC ← vns).
//!
//! RAM path: `resolve_pages` returns the server-side-flattened full page
//! list (root + delta chain — the engine never walks parents itself) and
//! the engine writes payloads through the slot's existing guest mapping,
//! which IS the memfd mmap (§8.3 step 2's "pwritev into the mapping").
//! The materialized-file `mmap(MAP_PRIVATE)` fast path is a perf
//! optimization measured by the perf-gate bead (9sb); it changes how bytes
//! arrive, not what this engine guarantees.
//!
//! Restore-time clock seam: the new segment starts at icount 0
//! (segment-relative, §3.1 — the caller's counter is re-zeroed here via
//! `PERF_EVENT_IOC_RESET` when provided), so guest time stays monotone only
//! if `PvClock::vns_base` becomes the boundary's absolute vns. That value
//! lives in the TIME section, NOT in CLKD (a segment's own base would be
//! stale by construction) — the engine reaches the concrete `PvClock`
//! through the `DetDevice::as_any_mut` downcast seam.
//!
//! Shape strictness: the container must carry exactly the sections this
//! bus would produce (`take_snapshot`'s fixed layout — MCFG, VCPU, LAPC,
//! TIME, ENTR + one section per non-entropy bus device). Anything else is
//! a loud error: restoring a snapshot into a differently-shaped machine is
//! a determinism bug, never a "best effort". Two deliberate special
//! shapes: LAPC is EXPECTED to be an empty v1 section (no lapic-stub
//! struct exists yet; a future struct is a sec_version bump on both
//! sides), and ENTR v2 splits into the VMM-owned PRNG
//! (`DetEntropy::restore`) + the pv-entropy device regs fed to
//! `device.restore(&regs, 1)` — the DEVICE's version 1, never the
//! section's 2 (the 6yl version-domain split).

use dh_detclock::counter::InstRetired;
use dh_devices::clock::{PvClock, DEVICE_ID_PV_CLOCK};
use dh_devices::entropy::{DetEntropy, EntropyState, DEVICE_ID_PV_ENTROPY};
use dh_snapshot::dhsnap::{tag, Container, EntrSectionV2, TimeSection};
use dh_vmm::config::MachineConfig;
use dh_vmm::dirty::{DirtyPageSet, PAGE_SIZE};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::SlotVm;
use dh_vmm::{vcpu_state, SlotState};
use snapstore_client::blocking::SnapstoreClient;
use snapstore_types::SnapshotRef;
use vm_memory::{Bytes, GuestAddress};

use crate::snapshot_engine::DEVICE_BLOB_FORMAT_DHSNAP;

/// The engine-owned sections every container carries — MCFG, VCPU, LAPC,
/// TIME, ENTR (`build_dhsnap`'s fixed layout; capture and restore must
/// agree, so a change there changes this).
const FIXED_ENGINE_SECTIONS: usize = 5;

#[derive(Debug)]
pub enum RestoreError {
    /// Slot must be Paused (created, not running) to be stuffed.
    NotPaused { state: SlotState },
    /// The snapshot was taken on a different machine shape: MCFG bytes or
    /// guest RAM size disagree with what the caller built the slot from.
    ConfigMismatch(String),
    /// KVM-side write/restore failure.
    Kvm(String),
    /// Container/section decode failure, or a section↔bus shape mismatch.
    Codec(String),
    /// Store failure — nothing was restored reliably; the slot must be
    /// treated as scrap (partial RAM writes may have landed).
    Store(String),
}

/// What the caller (run control / the slot table) needs to resume the
/// segment: the boundary position, the resumed hash chain, and the slot's
/// new VMM-owned PRNG. Segment-relative icount is 0 by definition;
/// `cumulative_icount` is the continuing total for §3.1 accounting.
pub struct RestoreOutcome {
    /// Total guest RAM pages materialized — always `guest_ram_bytes / 4096`
    /// (the server-flattened set covers every page), NOT a wire/delta
    /// count; compare `TakeSnapshotOutcome::pages_shipped`.
    pub pages_loaded: u64,
    pub cumulative_icount: u64,
    pub vns: u64,
    pub epoch_index: u64,
    /// `StateHashChain::from_value(TIME.hash_chain)` — the child segment
    /// continues the parent's chain, not a fresh `H_0`.
    pub chain: StateHashChain,
    /// The restored §5 PRNG. Replaces the slot's `DetEntropy` wholesale.
    pub entropy: DetEntropy,
}

/// Recover the `MachineConfig` embedded in a snapshot's DHSNAP `MCFG`
/// section. `dh-workerd` uses this before restore so
/// `RestoreSnapshotResponse.config` comes from the snapshot itself, not
/// caller-supplied memory.
pub fn recover_machine_config(
    snapshot_ref: SnapshotRef,
    store: &SnapstoreClient,
) -> Result<MachineConfig, RestoreError> {
    let container_bytes = store
        .get_snapshot(snapshot_ref)
        .map_err(|e| RestoreError::Store(format!("get_snapshot: {e}")))?;
    let manifest = snapstore_manifest::Manifest::decode(&container_bytes)
        .map_err(|e| RestoreError::Codec(format!("manifest: {e}")))?;
    recover_machine_config_from_blob(&manifest.device_blob)
}

/// One RestoreSnapshot, end to end. On success the slot holds exactly the
/// snapshot's state, Paused at a boundary with segment-relative icount 0.
/// On error the slot's contents are UNDEFINED (RAM, devices, or vCPU may
/// be partially written) — the caller must discard it, never resume it.
///
/// PRECONDITION beyond the state check: the slot must be FRESH — created
/// and never run. `Paused` alone does not prove that: a previously-Running
/// slot can hold stale KVM dirty-RING entries this engine does not drain
/// (the host-side RAM writes below bypass KVM's dirty tracking entirely),
/// which would poison the next incremental snapshot. Same-slot reuse is
/// the slot manager's job: drain + reset dirty tracking first.
#[allow(clippy::too_many_arguments)]
pub fn restore_snapshot(
    slot: &SlotVm,
    slot_state: SlotState,
    bus: &mut dh_devices::MmioBus,
    machine_config: &dh_vmm::config::MachineConfig,
    snapshot_ref: SnapshotRef,
    counter: Option<&InstRetired>,
    dirty: Option<&mut DirtyPageSet>,
    store: &SnapstoreClient,
) -> Result<RestoreOutcome, RestoreError> {
    if slot_state != SlotState::Paused {
        return Err(RestoreError::NotPaused { state: slot_state });
    }

    // ── 1. Manifest (§8.3 step 1): fetch + shape checks ──────────────────
    let container_bytes = store
        .get_snapshot(snapshot_ref.clone())
        .map_err(|e| RestoreError::Store(format!("get_snapshot: {e}")))?;
    let manifest = snapstore_manifest::Manifest::decode(&container_bytes)
        .map_err(|e| RestoreError::Codec(format!("manifest: {e}")))?;
    if manifest.guest_ram_bytes != slot.mem_bytes {
        return Err(RestoreError::ConfigMismatch(format!(
            "snapshot guest RAM is {} bytes, slot has {}",
            manifest.guest_ram_bytes, slot.mem_bytes
        )));
    }
    let blob = &manifest.device_blob;
    validate_dhsnap_blob(blob)?;

    // ── 2. RAM (§8.3 step 2): flattened pages → the live mapping ─────────
    // mem_bytes is page-multiple by SlotVm construction; keep the
    // truncating division's invariant loud (mirrors snapshot_engine).
    debug_assert!(slot.mem_bytes.is_multiple_of(PAGE_SIZE));
    let total_pages = slot.mem_bytes / PAGE_SIZE;
    let resolved = store
        .resolve_pages(snapshot_ref, None, false)
        .map_err(|e| RestoreError::Store(format!("resolve_pages: {e}")))?;
    let mut covered = vec![false; total_pages as usize];
    for (idx, _hash, payload) in &resolved {
        if *idx >= total_pages || covered[*idx as usize] {
            return Err(RestoreError::Store(format!(
                "resolved page {idx} out of range or duplicated"
            )));
        }
        let bytes = payload
            .as_ref()
            .ok_or_else(|| RestoreError::Store(format!("page {idx} arrived without payload")))?;
        if bytes.len() != PAGE_SIZE as usize {
            return Err(RestoreError::Store(format!(
                "page {idx} payload is {} bytes",
                bytes.len()
            )));
        }
        slot.guest_mem
            .write_slice(bytes, GuestAddress(idx * PAGE_SIZE))
            .map_err(|e| RestoreError::Kvm(format!("page {idx} write: {e}")))?;
        covered[*idx as usize] = true;
    }
    if covered.iter().any(|c| !c) {
        // A flattened chain bottoms out in a FULL manifest, which covers
        // every page by invariant — a hole means a broken store, not a
        // sparse snapshot.
        return Err(RestoreError::Store(
            "resolved page set does not cover guest RAM".into(),
        ));
    }

    // ── 3-6: the RAM-independent half (shared with the fork engine) ──────
    let applied = apply_dhsnap(slot, bus, machine_config, &blob.bytes, counter, dirty)?;
    Ok(RestoreOutcome {
        pages_loaded: total_pages,
        cumulative_icount: applied.cumulative_icount,
        vns: applied.vns,
        epoch_index: applied.epoch_index,
        chain: applied.chain,
        entropy: applied.entropy,
    })
}

/// The machine state a DHSNAP application yields (everything in
/// [`RestoreOutcome`] except the RAM accounting).
pub(crate) struct AppliedMachine {
    pub cumulative_icount: u64,
    pub vns: u64,
    pub epoch_index: u64,
    pub chain: StateHashChain,
    pub entropy: DetEntropy,
}

fn validate_dhsnap_blob(blob: &snapstore_manifest::DeviceBlob) -> Result<(), RestoreError> {
    if blob.format != DEVICE_BLOB_FORMAT_DHSNAP {
        return Err(RestoreError::Codec(format!(
            "device blob format {:#010x} is not DHSNAP ({DEVICE_BLOB_FORMAT_DHSNAP:#010x})",
            blob.format
        )));
    }
    if blob.zstd || blob.raw_len != blob.bytes.len() as u64 {
        return Err(RestoreError::Codec(
            "device blob is compressed or length-inconsistent (engine writes plain)".into(),
        ));
    }
    Ok(())
}

fn recover_machine_config_from_blob(
    blob: &snapstore_manifest::DeviceBlob,
) -> Result<MachineConfig, RestoreError> {
    validate_dhsnap_blob(blob)?;
    recover_machine_config_from_dhsnap(&blob.bytes)
}

fn recover_machine_config_from_dhsnap(dhsnap_bytes: &[u8]) -> Result<MachineConfig, RestoreError> {
    let dhsnap = Container::parse(dhsnap_bytes)
        .map_err(|e| RestoreError::Codec(format!("DHSNAP: {e:?}")))?;
    let mcfg = dhsnap
        .get(tag::MCFG)
        .ok_or_else(|| RestoreError::Codec("missing MCFG section".into()))?;
    decode_machine_config_section(mcfg)
}

fn decode_machine_config_section(
    section: &dh_snapshot::dhsnap::Section<'_>,
) -> Result<MachineConfig, RestoreError> {
    if section.sec_version != 1 {
        return Err(RestoreError::Codec(format!(
            "MCFG v{} is not supported",
            section.sec_version
        )));
    }
    MachineConfig::canonical_decode(section.contents)
        .map_err(|e| RestoreError::Codec(format!("MCFG decode: {e:?}")))
}

/// Steps 3–6 of §8.3 — decode the DHSNAP and stuff devices + vCPU into a
/// slot whose RAM is ALREADY the snapshot's bytes (materialized from the
/// store on the tier-B path, or CoW-shared with a frozen parent on the
/// tier-A fork path; the precondition is the caller's because device
/// restore may validate against live guest RAM).
pub(crate) fn apply_dhsnap(
    slot: &SlotVm,
    bus: &mut dh_devices::MmioBus,
    machine_config: &dh_vmm::config::MachineConfig,
    dhsnap_bytes: &[u8],
    counter: Option<&InstRetired>,
    dirty: Option<&mut DirtyPageSet>,
) -> Result<AppliedMachine, RestoreError> {
    // ── 3. DHSNAP decode + the fixed engine sections ──────────────────────
    let dhsnap = Container::parse(dhsnap_bytes)
        .map_err(|e| RestoreError::Codec(format!("DHSNAP: {e:?}")))?;
    let section = |t: [u8; 4]| {
        dhsnap.get(t).ok_or_else(|| {
            RestoreError::Codec(format!(
                "missing {} section",
                core::str::from_utf8(&t).unwrap_or("????")
            ))
        })
    };

    // MCFG: identity check against the config the caller built the slot
    // from. The engine refuses to guess: a mismatch means this snapshot
    // belongs to a different machine.
    let mcfg = section(tag::MCFG)?;
    decode_machine_config_section(mcfg)?;
    let expected = machine_config
        .canonical_encode()
        .map_err(|e| RestoreError::Codec(format!("MCFG encode: {e:?}")))?;
    if mcfg.contents != expected.as_slice() {
        return Err(RestoreError::ConfigMismatch(
            "MCFG does not match the slot's MachineConfig".into(),
        ));
    }

    let t = section(tag::TIME)?;
    let time = TimeSection::decode(t.contents, t.sec_version)
        .map_err(|e| RestoreError::Codec(format!("TIME: {e:?}")))?;

    // LAPC: empty v1 IS the expected shape (capture writes it that way —
    // no in-kernel irqchip, injection state lives in run control). Anything
    // else came from a newer writer this engine cannot restore.
    let lapc = section(tag::LAPC)?;
    if lapc.sec_version != 1 || !lapc.contents.is_empty() {
        return Err(RestoreError::Codec(format!(
            "LAPC v{} with {} bytes — this engine only restores the empty v1 placeholder",
            lapc.sec_version,
            lapc.contents.len()
        )));
    }

    let e = section(tag::ENTR)?;
    let entr = EntrSectionV2::decode(e.contents, e.sec_version)
        .map_err(|e| RestoreError::Codec(format!("ENTR (engine requires v2): {e:?}")))?;

    // ── 4. Devices (§8.3: RAM is live now) ────────────────────────────────
    // Shape checks FIRST, before any device mutates: exactly one pv-entropy
    // device (its regs come out of ENTR v2 — zero means no consumer, two
    // means an ambiguous one, and the section count cannot catch either),
    // and the container must carry exactly the sections this bus consumes.
    // Per-device presence is still checked in the loop; with the count
    // equality that also rules out sections with no device on this bus
    // (the parser already rejects duplicate and unknown tags).
    let entropy_devices = bus
        .devices()
        .filter(|(_b, d)| d.device_id() == DEVICE_ID_PV_ENTROPY)
        .count();
    if entropy_devices != 1 {
        return Err(RestoreError::Codec(format!(
            "bus has {entropy_devices} pv-entropy devices (0x0004), need exactly one"
        )));
    }
    let non_entropy_devices = bus.devices().count() - 1;
    let total_sections = dhsnap.sections().count();
    if total_sections != FIXED_ENGINE_SECTIONS + non_entropy_devices {
        return Err(RestoreError::Codec(format!(
            "container has {total_sections} sections but this bus consumes {} — \
             snapshot was taken on a differently-shaped machine",
            FIXED_ENGINE_SECTIONS + non_entropy_devices
        )));
    }

    for (_base, dev) in bus.devices_mut() {
        let id = dev.device_id();
        if id == DEVICE_ID_PV_ENTROPY {
            // The 6yl split: device regs come out of ENTR v2, restored at
            // the DEVICE's own section version.
            let regs = entr.device_regs();
            dev.restore(&regs, 1).map_err(|_| {
                RestoreError::Codec(format!(
                    "pv-entropy device rejected the ENTR v2 reg blob \
                     ({} bytes at device sec_version 1)",
                    regs.len()
                ))
            })?;
            continue;
        }
        let dev_tag = dh_snapshot::dhsnap::tag_for_device_id(id)
            .ok_or_else(|| RestoreError::Codec(format!("device id {id:#06x} has no DHSNAP tag")))?;
        let s = dhsnap.get(dev_tag).ok_or_else(|| {
            RestoreError::Codec(format!(
                "bus device {id:#06x} has no section in the container"
            ))
        })?;
        dev.restore(s.contents, s.sec_version).map_err(|_| {
            RestoreError::Codec(format!(
                "device {id:#06x} rejected its section (v{}, {} bytes)",
                s.sec_version,
                s.contents.len()
            ))
        })?;
        // PvClock vns_base ← TIME.vns, right after its own section restore
        // (see the module doc's clock seam note) — the downcast seam
        // appears exactly once, on the same walk.
        if id == DEVICE_ID_PV_CLOCK {
            let clk = dev
                .as_any_mut()
                .and_then(|a| a.downcast_mut::<PvClock>())
                .ok_or_else(|| {
                    RestoreError::Codec("clock device does not downcast to PvClock".into())
                })?;
            clk.set_vns_base(time.vns);
        }
    }

    // ── 5. vCPU last (§8.3; KVM_SET_* order owned by vcpu_state) ─────────
    let v = section(tag::VCPU)?;
    let st = vcpu_state::decode_section(v.contents, v.sec_version)
        .map_err(|e| RestoreError::Codec(format!("VCPU: {e:?}")))?;
    vcpu_state::restore(slot, &st, time.vns)
        .map_err(|e| RestoreError::Kvm(format!("vCPU restore: {e:?}")))?;

    // ── 6. Segment re-seed: counter ← 0, dirty set cleared ────────────────
    if let Some(c) = counter {
        c.reset()
            .map_err(|e| RestoreError::Kvm(format!("counter reset: {e:?}")))?;
    }
    if let Some(d) = dirty {
        d.clear();
    }

    Ok(AppliedMachine {
        cumulative_icount: time.cumulative_icount,
        vns: time.vns,
        epoch_index: time.epoch_index,
        chain: StateHashChain::from_value(time.hash_chain),
        entropy: DetEntropy::restore(EntropyState {
            seed: entr.seed,
            stream: entr.stream,
            word_pos: entr.word_pos,
        }),
    })
}
