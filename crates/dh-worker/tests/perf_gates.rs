//! M4 ACCEPT perf gates (bead 9sb): p50 latency thresholds on the
//! quiesced Intel box, 128 MiB guest (the MAP.md canonical demo figure).
//! Thresholds are the ACCEPTED-AS-MEASURED numbers (bead 8ot, ledger
//! #20), not the plan's original aspirational ones — see the constants
//! below for the decision record.
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

// ACCEPTED-AS-MEASURED gates (bead 8ot decision, 2026-06-12; ledger #20):
// the box's storage sustains ~350 MB/s durable, so the plan's original
// snapshot/restore numbers (15 ms / 150 ms — they imply > 2 GB/s durable)
// were accepted at the measured baseline plus ~45% day-to-day variance
// headroom (measured p50: fork 326 µs, snapshot 103 ms, restore 307 ms).
// These are REGRESSION gates at the accepted baseline; the original
// numbers remain the improvement targets (correctness outranks speed).
const FORK_P50_MAX: Duration = Duration::from_millis(10);
const SNAP_P50_MAX: Duration = Duration::from_millis(150);
const RESTORE_P50_MAX: Duration = Duration::from_millis(450);

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

/// Deliberately the UPPER median for even sample counts — conservative
/// for a gate; do not "fix" into an averaging median (it loosens it).
fn p50(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// (min, p50, max) — the spread lets the operator spot a bimodal
/// distribution (cold/warm split, fsync hiccups) at a glance.
fn spread(samples: &mut [Duration]) -> (Duration, Duration, Duration) {
    let mid = p50(samples);
    (samples[0], mid, samples[samples.len() - 1])
}

#[test]
#[ignore = "M4 perf acceptance: quiesced box only — cargo test -p dh-worker --test perf_gates --release -- --ignored --nocapture"]
fn m4_perf_gates_p50_128mib() {
    // A skip looks exactly like a pass to a CI consumer. The nightly
    // perf job (bead 1pa) sets PERF_GATE_REQUIRED=1 so a misconfigured
    // runner fails loudly instead of going green without measuring;
    // ad-hoc operator runs keep the friendly skip.
    let required = std::env::var_os("PERF_GATE_REQUIRED").is_some();
    if !kvm_available() {
        assert!(!required, "PERF_GATE_REQUIRED set but /dev/kvm not usable");
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    if cfg!(debug_assertions) {
        assert!(
            !required,
            "PERF_GATE_REQUIRED set but this is a debug build (use --release)"
        );
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
            None,
            &mut bus_c,
            None,
        )
        .expect("fork");
        fork_samples.push(t.elapsed());
        drop(outcome); // child teardown outside the timed window of the NEXT sample
    }
    let (fork_min, fork_p50, fork_max) = spread(&mut fork_samples);
    eprintln!(
        "fork p50: {fork_p50:?} [min {fork_min:?}, max {fork_max:?}] (gate {FORK_P50_MAX:?})"
    );

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

    // Methodology (iteration-99 review): the SET — host-built, not guest
    // execution — defines the 8k load. Two deliberate deviations from a
    // guest-dirtied run, both currently sub-ms next to 32 MiB of durable
    // I/O (revisit if the path ever stops being storage-bound, bead 8ot):
    //  - harvest_at_boundary drains an EMPTY ring here (a real run would
    //    harvest 8192 ring entries + reset ioctls inside the window);
    //  - page CONTENT varies per sample (sample index mixed in), because
    //    the pagestore dedups globally by content hash — identical bytes
    //    would make samples 2..N dedup hits and measure the manifest
    //    path, not the cold 8k-page write. Cost: the tempdir store grows
    //    ~32 MiB per sample (~1 GiB for the run).
    let mut snap_samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        for page in 0..DIRTY_PAGES {
            slot.guest_mem
                .write_slice(
                    &[(page as u8) ^ (sample as u8) ^ 0x5A],
                    GuestAddress(page * 4096),
                )
                .unwrap();
            // The engine clears the set after the store acks — rebuild.
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
        assert_eq!(
            out.pages_shipped, DIRTY_PAGES,
            "the load must be exactly 8k pages"
        );
    }
    let (snap_min, snap_p50, snap_max) = spread(&mut snap_samples);
    eprintln!(
        "incremental snapshot (8k pages) p50: {snap_p50:?} [min {snap_min:?}, max {snap_max:?}] (gate {SNAP_P50_MAX:?})"
    );

    // ── Gate 3: tier-B warm restore of the 128 MiB root ─────────────────
    // Slot creation is NOT part of the gate (RestoreSnapshot targets an
    // existing slot, §8.3) — created per sample outside the timed window.
    // Warm page cache across samples is the CORRECT regime: "tier-B WARM
    // restore" is the plan's wording; sample 0's cold read is an outlier
    // the median is robust to.
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
    let (restore_min, restore_p50, restore_max) = spread(&mut restore_samples);
    eprintln!(
        "warm restore p50: {restore_p50:?} [min {restore_min:?}, max {restore_max:?}] (gate {RESTORE_P50_MAX:?})"
    );

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
