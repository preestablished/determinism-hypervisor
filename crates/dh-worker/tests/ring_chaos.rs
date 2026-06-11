//! M4 ACCEPT: dirty-ring-full chaos (bead 28i; risk R8). The same
//! snapshot roundtrip on two slots that differ ONLY in dirty-ring size —
//! production 65536 entries vs the SMALLEST LEGAL ring — while the
//! page_dirtier guest writes 3072 pages, overflowing the small ring 3
//! times. Two empirics pin the test's parameters: (a) the bead said
//! "ring size 512", but the kernel reserves 64 + 512 (PML) entries on
//! x86 and rejects rings below that floor with EINVAL — 1024 is the
//! smallest ring this hardware accepts; (b) the slot stays at 16 MiB
//! because a 32 MiB FULL snapshot hangs the blocking store client
//! today (bead 0vl). Ring-full exits are host-visible only and
//! harvest-on-full is
//! loss-free by construction (§8.2): the two legs must produce the
//! IDENTICAL incremental snapshot ref (a single lost dirty page would
//! change the delta manifest), the same pages_shipped, and bit-equal
//! vCPU state — with a non-vacuity pin that the small ring actually
//! overflowed many times and the large ring never did.
//!
//! REF EQUALITY vs THE BEAD'S H1==H2 WORDING: the snapshot ref is
//! BLAKE3 over the manifest body, which folds in the page table
//! (content + indices) AND the DHSNAP device blob (vCPU + devices) —
//! so delta-ref equality is an equal-or-stronger discharge of the R8
//! claim than the state-hash comparison the bead sketched, minus the
//! restore-replay leg (R8 is about page LOSS, which the manifest
//! catches by construction; restorability is the 9wa/7c8 suites' job).
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

#![cfg(target_arch = "x86_64")]

mod common;

use common::{kvm_available, spawn_store_blocking, test_bus};
use dh_devices::entropy::DetEntropy;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::dirty::{enable_dirty_logging, harvest_at_boundary, DirtyPageSet, DirtyRing};
use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem, DIRTY_RING_ENTRIES};
use dh_vmm::{vcpu_state, SlotState};
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource, TakeSnapshotOutcome};
use snapstore_client::blocking::SnapstoreClient;
use snapstore_types::SnapshotRef;

const MEM: u64 = 16 << 20;
const SMALL_RING: u64 = 1024;

fn config() -> MachineConfig {
    MachineConfig::new(
        MEM,
        [0x88; 32],
        BootSpec::Elf {
            kernel_hash: [0x88; 32],
            cmdline: Vec::new(),
        },
    )
}

fn boundary() -> BoundaryState {
    BoundaryState {
        icount: 5_000_000,
        vns: 5_000_000,
        epoch_index: 0,
        hash_chain: [0xBB; 32],
        agenda_empty: true,
    }
}

struct LegOutcome {
    root: SnapshotRef,
    delta: TakeSnapshotOutcome,
    ring_full_exits: u64,
    vcpu: dh_vmm::vcpu_state::VcpuState,
}

/// Boot page_dirtier on a slot with `ring_entries`, take a FULL root,
/// run the guest to its park (servicing ring-full by harvesting), then
/// take the incremental delta.
fn run_leg(sys: &KvmSystem, store: &SnapstoreClient, ring_entries: u64) -> LegOutcome {
    let mut slot = sys
        .create_slot_vm_with_ring(MEM, ring_entries)
        .unwrap_or_else(|e| panic!("ring={ring_entries}: {e:?}"));
    dh_vmm::boot::load_and_enter(&slot, nanokernel::page_dirtier_elf(), b"").unwrap();
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x28; 32]);
    let cfg = config();

    let root = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &cfg,
        boundary(),
        PageSource::Full,
        store,
    )
    .expect("root snapshot")
    .snapshot_ref;

    let mut ring = DirtyRing::map(&slot).expect("ring map");
    let mut dirty = DirtyPageSet::new(slot.mem_bytes);
    enable_dirty_logging(&slot).expect("logging on");

    let mut ring_full_exits = 0u64;
    loop {
        match classify_exit(slot.vcpu.run().unwrap()) {
            ExitEvent::Hlt => break,
            ExitEvent::DirtyRingFull => {
                ring_full_exits += 1;
                harvest_at_boundary(&mut ring, &slot.vm, &mut dirty).unwrap();
            }
            other => panic!("ring={ring_entries}: unexpected exit {other:?}"),
        }
    }

    let delta = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &cfg,
        boundary(),
        PageSource::Incremental {
            parent: root.clone(),
            ring: &mut ring,
            dirty: &mut dirty,
        },
        store,
    )
    .expect("incremental snapshot");

    LegOutcome {
        root,
        delta,
        ring_full_exits,
        vcpu: vcpu_state::capture(&slot).unwrap(),
    }
}

#[test]
fn tiny_ring_chaos_changes_nothing_the_snapshot_can_see() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();

    let large = run_leg(&sys, &store, DIRTY_RING_ENTRIES);
    let small = run_leg(&sys, &store, SMALL_RING);

    // Non-vacuity (R8's stressor really fired): 3072 dirtied pages
    // overflow a 1024-entry ring repeatedly and a 65536-entry ring never.
    assert_eq!(large.ring_full_exits, 0, "large ring must not overflow");
    assert!(
        small.ring_full_exits >= 2,
        "small ring overflowed only {} times — not a chaos run",
        small.ring_full_exits
    );

    // Identical machines produce identical roots (sanity).
    assert_eq!(large.root, small.root);

    // THE acceptance: the delta refs match — every dirty page the guest
    // produced was harvested on BOTH legs (one lost page on the chaos
    // leg would change the delta manifest and therefore the ref), and
    // the guest's execution was unperturbed by the extra exits.
    assert_eq!(
        small.delta.snapshot_ref, large.delta.snapshot_ref,
        "ring-full chaos changed the snapshot"
    );
    assert_eq!(small.delta.pages_shipped, large.delta.pages_shipped);
    assert!(
        small.delta.pages_shipped >= nanokernel::PAGE_DIRTIER_PAGES,
        "delta smaller than the pages the guest wrote: {}",
        small.delta.pages_shipped
    );
    assert_eq!(small.vcpu, large.vcpu, "guest state diverged");
}
