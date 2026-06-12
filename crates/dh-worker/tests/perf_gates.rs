//! M4 ACCEPT perf gates (bead 9sb): p50 latency thresholds on the
//! quiesced Intel box, 128 MiB guest (the MAP.md canonical demo figure),
//! per IMPLEMENTATION-PLAN: tier-A fork < 10 ms, incremental snapshot at
//! 8k dirty pages < 15 ms, tier-B warm restore < 150 ms.
//!
//! ONE sequential test, #[ignore]d: perf assertions flake under parallel
//! suite load (the iteration-68/69 lesson), so this never runs in the
//! ordinary `cargo test` sweep — the nightly perf job (bead 1pa) and the
//! operator run it deliberately on the quiesced box:
//!
//!   cargo test -p dh-worker --test perf_gates --release -- --ignored --nocapture
//!
//! RELEASE MATTERS: the engines move 32 MiB–128 MiB per operation; debug
//! builds measure the compiler, not the platform. The test refuses to
//! gate a debug build (skips loudly) for the same reason.
//!
//! p50, not max: the gate is the IMPLEMENTATION-PLAN's median figure —
//! tail outliers (store fsync hiccups, scheduler noise) are the nightly
//! regression job's business (>20% drift), not this acceptance's.

#![cfg(target_arch = "x86_64")]

mod common;

use std::time::{Duration, Instant};

use common::{kvm_available, spawn_store_blocking, test_bus};
use dh_devices::entropy::DetEntropy;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::dirty::{enable_dirty_logging, DirtyPageSet, DirtyRing};
use dh_vmm::kvm::KvmSystem;
use dh_vmm::SlotState;
use dh_worker::fork_engine::fork_slot;
use dh_worker::restore_engine::restore_snapshot;
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use vm_memory::{Bytes, GuestAddress};

/// The MAP.md canonical demo-guest size.
const MEM: u64 = 128 << 20;
/// The IMPLEMENTATION-PLAN incremental-snapshot load.
const DIRTY_PAGES: u64 = 8192;
/// Samples per gate; the median of 30 is stable on the quiesced box.
const SAMPLES: usize = 30;

const FORK_P50_MAX: Duration = Duration::from_millis(10);
const SNAP_P50_MAX: Duration = Duration::from_millis(15);
const RESTORE_P50_MAX: Duration = Duration::from_millis(150);

fn config_128() -> MachineConfig {
    MachineConfig::new(
        MEM,
        [0x11; 32],
        BootSpec::Elf {
            kernel_hash: [0x22; 32],
            cmdline: b"console=none".to_vec(),
        },
    )
}

fn boundary() -> BoundaryState {
    BoundaryState {
        icount: 1_000_000,
        vns: 1_000_000,
        epoch_index: 2,
        hash_chain: [0xCA; 32],
        agenda_empty: true,
    }
}

fn p50(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
#[ignore = "M4 perf acceptance: quiesced box only — cargo test -p dh-worker --test perf_gates --release -- --ignored --nocapture"]
fn m4_perf_gates_p50_128mib() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    if cfg!(debug_assertions) {
        eprintln!("skipping: perf gates are meaningless in a debug build (use --release)");
        return;
    }

    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = config_128();

    // ── Gate 1: tier-A fork of a frozen 128 MiB parent ──────────────────
    let parent = sys.create_slot_vm(MEM).unwrap();
    // Recognizable, non-zero RAM so the memfd carries real content.
    parent
        .guest_mem
        .write_slice(&[0xAB; 4096], GuestAddress(0x10_0000))
        .unwrap();
    let bus_p = test_bus();
    let entropy_p = DetEntropy::from_seed([0x42; 32]);
    parent.freeze_ram().expect("freeze parent RAM");

    let mut fork_samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let mut bus_c = test_bus();
        let t = Instant::now();
        let outcome = fork_slot(
            &sys,
            &parent,
            SlotState::Frozen,
            &bus_p,
            &entropy_p,
            &config,
            boundary(),
            &mut bus_c,
            None,
        )
        .expect("fork");
        fork_samples.push(t.elapsed());
        drop(outcome); // child teardown outside the timed window of the NEXT sample
    }
    let fork_p50 = p50(&mut fork_samples);
    eprintln!("fork p50: {fork_p50:?} (gate {FORK_P50_MAX:?})");

    // ── Gate 2: incremental snapshot at exactly 8k dirty pages ──────────
    let slot = sys.create_slot_vm(MEM).unwrap();
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x43; 32]);
    let root = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("root snapshot");

    let mut ring = DirtyRing::map(&slot).expect("ring");
    let mut dirty = DirtyPageSet::new(slot.mem_bytes);
    enable_dirty_logging(&slot).expect("dirty logging");
    // Real bytes in every page the set will ship (host writes; the SET —
    // not guest execution — defines the 8k load, which is exactly the
    // engine path the gate times).
    for page in 0..DIRTY_PAGES {
        slot.guest_mem
            .write_slice(&[page as u8 ^ 0x5A], GuestAddress(page * 4096))
            .unwrap();
    }

    let mut snap_samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        // The engine clears the set after the store acks — rebuild it
        // (8192 bitset inserts; noise floor next to 32 MiB of I/O).
        for page in 0..DIRTY_PAGES {
            dirty.insert(page).unwrap();
        }
        let t = Instant::now();
        let out = take_snapshot(
            &slot,
            SlotState::Paused,
            &bus,
            &entropy,
            &config,
            boundary(),
            PageSource::Incremental {
                parent: root.snapshot_ref.clone(),
                ring: &mut ring,
                dirty: &mut dirty,
            },
            &store,
        )
        .expect("incremental snapshot");
        snap_samples.push(t.elapsed());
        assert_eq!(out.pages_shipped, DIRTY_PAGES, "the load must be exactly 8k pages");
    }
    let snap_p50 = p50(&mut snap_samples);
    eprintln!("incremental snapshot (8k pages) p50: {snap_p50:?} (gate {SNAP_P50_MAX:?})");

    // ── Gate 3: tier-B warm restore of the 128 MiB root ─────────────────
    // Slot creation is NOT part of the gate (RestoreSnapshot targets an
    // existing slot, §8.3) — created per sample outside the timed window.
    let mut restore_samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let fresh = sys.create_slot_vm(MEM).unwrap();
        let mut bus_f = test_bus();
        let t = Instant::now();
        let out = restore_snapshot(
            &fresh,
            SlotState::Paused,
            &mut bus_f,
            &config,
            root.snapshot_ref.clone(),
            None,
            None,
            &store,
        )
        .expect("restore");
        restore_samples.push(t.elapsed());
        assert_eq!(out.pages_loaded, MEM / 4096);
    }
    let restore_p50 = p50(&mut restore_samples);
    eprintln!("warm restore p50: {restore_p50:?} (gate {RESTORE_P50_MAX:?})");

    // ── The gates ────────────────────────────────────────────────────────
    assert!(
        fork_p50 < FORK_P50_MAX,
        "M4 gate: fork p50 {fork_p50:?} >= {FORK_P50_MAX:?}"
    );
    assert!(
        snap_p50 < SNAP_P50_MAX,
        "M4 gate: incremental snapshot p50 {snap_p50:?} >= {SNAP_P50_MAX:?}"
    );
    assert!(
        restore_p50 < RESTORE_P50_MAX,
        "M4 gate: warm restore p50 {restore_p50:?} >= {RESTORE_P50_MAX:?}"
    );
}
