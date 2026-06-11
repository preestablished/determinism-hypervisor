//! TakeSnapshot orchestration (bead qmp; ARCH §8.2): land on a boundary →
//! drain the dirty ring → ship pages to the REAL snapshot-store →
//! assemble DHSNAP → `PutSnapshot` → return the ref ONLY after the store
//! has durably acknowledged it (R12: the ref IS the durability receipt).
//!
//! The engine is synchronous: the store is reached through
//! `snapstore_client::blocking::SnapstoreClient` (the sibling's sync-async
//! bridge for vCPU worker loops — docs/decisions/
//! snapstore-server-for-tests.md). The caller (run control / the slot
//! table) guarantees and ATTESTS the §8.1 preconditions via
//! [`BoundaryState`]: vCPU stopped at a deterministic boundary, agenda
//! empty. The engine re-checks what it can and fails loudly otherwise.
//!
//! HASH vs SECTION reconciliation (iteration-70 review I1, decided here):
//! the two vCPU artifacts STAY SEPARATE — option (b). The state-hash
//! preimage keeps hash.rs's field-selective `canonical_vcpu_blob`
//! (padding-excluded by construction, the Phase-1 discipline every
//! existing chain value depends on); the DHSNAP `VCPU` section is
//! `vcpu_state::encode_section` (API.md §4 raw structs, restore-capable).
//! Folding the raw-struct bytes into the hash would re-import the
//! reserved-byte hazard class that iteration 69 eliminated, for no
//! verification gain — the chain already covers the same logical state.
//! ARCH §8.1's "canonical vCPU blob (DHSNAP vCPU section bytes)" line is
//! therefore stale wording; veu divergence #8 tracks the upstream fix.
//!
//! Section presence (v1): MCFG, VCPU, TIME, ENTR (v2: PRNG + device regs)
//! always; LAPC always, as an EMPTY v1 section (no lapic-stub struct
//! exists yet — no in-kernel irqchip, injection state lives in run
//! control; the struct lands with a later bead as a sec_version bump);
//! one section per bus device via the dhsnap id↔tag map (CLKD, PADD,
//! BLKO, SERL, EVTC…, NETL when bead mmv lands the device). The entropy
//! device (0x0004) is the documented special case: its reg blob folds
//! into ENTR v2 instead of being framed alone.

use dh_snapshot::dhsnap::{tag, ContainerWriter, EntrSection, EntrSectionV2, TimeSection};
use dh_vmm::dirty::{harvest_at_boundary, DirtyPageSet, DirtyRing, PAGE_SIZE};
use dh_vmm::kvm::SlotVm;
use dh_vmm::{vcpu_state, SlotState};
use snapstore_client::blocking::SnapstoreClient;
use snapstore_manifest::DeviceBlob;
use snapstore_types::SnapshotRef;
use vm_memory::{Bytes, GuestAddress};

/// `DeviceBlob.format` tag for DHSNAP containers (caller-defined per the
/// store's contract; pinned here, the one place).
pub const DEVICE_BLOB_FORMAT_DHSNAP: u32 = 0x4448_534E; // "DHSN"

/// The caller's attestation of the §8.1 snapshot preconditions plus the
/// boundary position that seeds the TIME section.
#[derive(Clone, Copy, Debug)]
pub struct BoundaryState {
    pub icount: u64,
    pub vns: u64,
    pub epoch_index: u64,
    /// Current StateHashChain value — recorded in TIME; the child segment
    /// resumes the chain from it (`StateHashChain::from_value`).
    pub hash_chain: [u8; 32],
    /// MUST be true: snapshots only happen at quiescent boundaries with no
    /// unconsumed scheduled events (§8.1).
    pub agenda_empty: bool,
}

/// What to snapshot: the page-set strategy.
pub enum PageSource<'a> {
    /// Root snapshot: every page, full walk (FULL manifest).
    Full,
    /// Incremental: drain the ring, send pages dirtied since the parent
    /// (DELTA manifest). The dirty set is CLEARED only after the store
    /// acks (§8.2 "clear dirty set" is the last step).
    Incremental {
        parent: SnapshotRef,
        ring: &'a mut DirtyRing,
        dirty: &'a mut DirtyPageSet,
    },
}

#[derive(Clone, Debug)]
pub struct TakeSnapshotOutcome {
    pub snapshot_ref: SnapshotRef,
    /// Pages shipped (== dirty count for incremental, total for full).
    pub pages_shipped: u64,
    /// The chain value to seed the child segment with.
    pub hash_chain: [u8; 32],
}

#[derive(Debug)]
pub enum EngineError {
    /// §8.1: agenda must be empty at the boundary.
    AgendaNotEmpty,
    /// Slot must be Paused (the d2p state machine's read of the world).
    NotPaused { state: SlotState },
    /// KVM-side capture/harvest failure.
    Kvm(String),
    /// Codec failure assembling DHSNAP.
    Codec(String),
    /// Store failure — the ref was NOT issued; nothing was cleared.
    Store(String),
}

/// One TakeSnapshot, end to end. On success the ref is durably stored;
/// for incremental sources the dirty set has been cleared.
#[allow(clippy::too_many_arguments)]
pub fn take_snapshot(
    slot: &SlotVm,
    slot_state: SlotState,
    bus: &dh_devices::MmioBus,
    entropy: &dh_devices::entropy::DetEntropy,
    machine_config: &dh_vmm::config::MachineConfig,
    boundary: BoundaryState,
    source: PageSource<'_>,
    store: &SnapstoreClient,
) -> Result<TakeSnapshotOutcome, EngineError> {
    if !boundary.agenda_empty {
        return Err(EngineError::AgendaNotEmpty);
    }
    if slot_state != SlotState::Paused {
        return Err(EngineError::NotPaused { state: slot_state });
    }

    // ── 1. Page set (§8.2: drain ring at the pause) ───────────────────────
    let total_pages = slot.mem_bytes / PAGE_SIZE;
    let (page_indices, parent, dirty_to_clear): (
        Vec<u64>,
        Option<SnapshotRef>,
        Option<&mut DirtyPageSet>,
    ) = match source {
        PageSource::Full => ((0..total_pages).collect(), None, None),
        PageSource::Incremental {
            parent,
            ring,
            dirty,
        } => {
            harvest_at_boundary(ring, &slot.vm, dirty)
                .map_err(|e| EngineError::Kvm(format!("harvest: {e:?}")))?;
            let indices: Vec<u64> = dirty.iter().collect();
            (indices, Some(parent), Some(dirty))
        }
    };

    // ── 2. Read pages from the live mapping (paused: no shadow copy) ──────
    let mut pages: Vec<(u64, Vec<u8>)> = Vec::with_capacity(page_indices.len());
    for idx in &page_indices {
        let mut buf = vec![0u8; PAGE_SIZE as usize];
        slot.guest_mem
            .read_slice(&mut buf, GuestAddress(idx * PAGE_SIZE))
            .map_err(|e| EngineError::Kvm(format!("page {idx} read: {e}")))?;
        pages.push((*idx, buf));
    }
    let pages_shipped = pages.len() as u64;

    // ── 3. Assemble DHSNAP ────────────────────────────────────────────────
    let dhsnap = build_dhsnap(slot, bus, entropy, machine_config, &boundary)?;

    // ── 4. Ship + manifest + PutSnapshot in one seam: the client's
    //       put_snapshot_from_parts uploads the bare page bytes FIRST
    //       (server hashes + dedups, client cross-checks batch_blake3 —
    //       the §8.2 order), then builds and puts the container. The
    //       returned ref is the durability receipt (R12). An empty
    //       incremental (no guest writes since the parent) is a VALID
    //       zero-page DELTA, not an error — verified against the store. ──
    let snapshot_ref = store
        .put_snapshot_from_parts(
            parent.as_ref(),
            slot.mem_bytes,
            pages,
            DeviceBlob {
                format: DEVICE_BLOB_FORMAT_DHSNAP,
                zstd: false,
                raw_len: dhsnap.len() as u64,
                bytes: dhsnap,
            },
        )
        .map_err(|e| EngineError::Store(format!("put_snapshot: {e}")))?;

    // ── 5. Only now: clear the dirty set (§8.2's last step) ───────────────
    if let Some(dirty) = dirty_to_clear {
        dirty.clear();
    }

    Ok(TakeSnapshotOutcome {
        snapshot_ref,
        pages_shipped,
        hash_chain: boundary.hash_chain,
    })
}

/// DHSNAP assembly in the canonical §4 table order (byte-determinism: the
/// container is part of the snapshot-ref preimage, so section order is
/// fixed HERE, not left to bus iteration order).
///
/// `pub(crate)`: the tier-A fork engine builds the parent's IN-MEMORY
/// DHSNAP through this exact assembler (§8.4 "decode the parent's
/// in-memory DHSNAP") — one codec, never a parallel fork-only encoding.
pub(crate) fn build_dhsnap(
    slot: &SlotVm,
    bus: &dh_devices::MmioBus,
    entropy: &dh_devices::entropy::DetEntropy,
    machine_config: &dh_vmm::config::MachineConfig,
    boundary: &BoundaryState,
) -> Result<Vec<u8>, EngineError> {
    let codec = |e: dh_snapshot::dhsnap::WriteError| EngineError::Codec(format!("{e:?}"));
    let mut w = ContainerWriter::new();

    // MCFG: the machine_config_hash preimage (restore recovers the config
    // from here — the store manifest carries no machine-config metadata).
    let mcfg = machine_config
        .canonical_encode()
        .map_err(|e| EngineError::Codec(format!("MCFG encode: {e:?}")))?;
    w.push_section(tag::MCFG, 1, &mcfg).map_err(codec)?;

    // VCPU: the API.md §4 raw-struct section (restore-capable; see the
    // module doc for the hash-vs-section split decision).
    let captured =
        vcpu_state::capture(slot).map_err(|e| EngineError::Kvm(format!("capture: {e:?}")))?;
    w.push_section(
        tag::VCPU,
        vcpu_state::VCPU_SECTION_VERSION,
        &vcpu_state::encode_section(&captured),
    )
    .map_err(codec)?;

    // LAPC: empty v1 — no lapic-stub struct exists yet (no in-kernel
    // irqchip; injection state lives in run control). sec_version bump
    // when the struct lands.
    w.push_section(tag::LAPC, 1, &[]).map_err(codec)?;

    // TIME: the boundary position + chain value.
    w.push_section(
        tag::TIME,
        TimeSection::VERSION,
        &TimeSection {
            cumulative_icount: boundary.icount,
            vns: boundary.vns,
            epoch_index: boundary.epoch_index,
            hash_chain: boundary.hash_chain,
        }
        .encode(),
    )
    .map_err(codec)?;

    // ENTR v2: VMM-owned PRNG state + the entropy DEVICE's reg blob (the
    // resolved 6yl landmine — never the device blob alone).
    let prng = entropy.state();
    let mut entropy_regs: Option<Vec<u8>> = None;

    // Walk the bus once, in registration (base-address) order — MmioBus
    // iteration is deterministic by construction (sorted bases).
    let mut device_sections: Vec<([u8; 4], u16, Vec<u8>)> = Vec::new();
    for (_base, dev) in bus.devices() {
        let mut contents = Vec::new();
        dev.snapshot(&mut contents);
        let id = dev.device_id();
        if id == 0x0004 {
            if entropy_regs.is_some() {
                return Err(EngineError::Codec(
                    "two pv-entropy devices (0x0004) on the bus — ENTR v2 holds one reg blob"
                        .into(),
                ));
            }
            entropy_regs = Some(contents);
            continue; // folded into ENTR v2 below
        }
        let tag = dh_snapshot::dhsnap::tag_for_device_id(id)
            .ok_or_else(|| EngineError::Codec(format!("device id {id:#x} has no DHSNAP tag")))?;
        device_sections.push((tag, dev.section_version(), contents));
    }

    let entropy_regs = entropy_regs
        .ok_or_else(|| EngineError::Codec("pv-entropy device (0x0004) not on the bus".into()))?;
    let v2 = EntrSectionV2::from_parts(
        EntrSection {
            seed: prng.seed,
            stream: prng.stream,
            word_pos: prng.word_pos,
        },
        &entropy_regs,
    )
    .map_err(|e| EngineError::Codec(format!("ENTR v2: {e:?}")))?;
    w.push_section(tag::ENTR, EntrSectionV2::VERSION, &v2.encode())
        .map_err(codec)?;

    // Remaining device sections, §4 table order (CLKD, PADD, EVTC, BLKO,
    // NETL, SERL) — sorted by the canonical KNOWN_TAGS position so two
    // engines with different bus layouts produce identical bytes for
    // identical state.
    device_sections.sort_by_key(|(tag, _, _)| {
        dh_snapshot::dhsnap::KNOWN_TAGS
            .iter()
            .position(|t| t == tag)
            .expect("mapped tags are always known")
    });
    for (tag, version, contents) in device_sections {
        w.push_section(tag, version, &contents).map_err(codec)?;
    }

    Ok(w.finish())
}
