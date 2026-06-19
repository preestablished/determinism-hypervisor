//! M4 ACCEPT: ENTR golden (bead dy8). Snapshot MID-STREAM while the
//! entropy_draw guest pulls 16-byte fills through the REAL pv-entropy
//! MMIO doorbell path, restore into a fresh slot, and the restored
//! machine's next 1024 draws must be byte-identical to the
//! un-snapshotted continuation — the {seed, stream, word_pos} tuple
//! (API.md §4 / ENTR v2) round-trips exactly, device regs included.
//!
//! The un-snapshotted continuation IS the golden: leg A keeps running
//! past the snapshot point and its draws (count_pause .. count_pause +
//! 1024) are the reference bytes. Leg B restores the snapshot, runs the
//! same batches, and must produce the same ring bytes AND leave its
//! DetEntropy at the same state. The counter axis is continuous across
//! all legs (same thread, never reset) — chain values are NOT compared
//! here (absolute icounts differ between legs by construction); the
//! draw bytes are the claim.
//!
//! BATCH BOUNDARIES, NOT LANDINGS: the guest HLTs every 256 draws and
//! each segment stops on GuestHalted — an exact, zero-skid exit.
//! Goal-grid polling was tried first and OVERSHOT: a PMI landing that
//! must single-step across this guest's MMIO instructions can lose the
//! single-step trap on an MMIO-write exit and free-run (~74 instrs
//! observed) past the target. That hazard is filed as its own bead;
//! this acceptance does not depend on landings at all.
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

#![cfg(target_arch = "x86_64")]

mod common;

use std::sync::atomic::AtomicBool;

use common::{gettid, kvm_available, spawn_store_blocking, test_bus, VmMem};
use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::entropy::DetEntropy;
use dh_devices::{DevCtx, MmioBus};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::runctl::{run_segment, Segment, SegmentOutcome, StopReason, Until};
use dh_vmm::SlotState;
use dh_worker::restore_engine::restore_snapshot;
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use kvm_ioctls::VcpuExit;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const MEM: u64 = 16 << 20;
/// Batches before the snapshot (2 x 256 = 512 draws mid-stream).
const BATCHES_BEFORE: u64 = 2;
/// Batches per continuation (4 x 256 = 1024 golden draws).
const BATCHES_GOLDEN: u64 = 4;
const GOLDEN_DRAWS: u64 = BATCHES_GOLDEN * nanokernel::ENTROPY_DRAW_BATCH;
const HUGE_BUDGET: u64 = 1_000_000_000;

/// m1_acceptance's GuestMem adapter, minimal: the device writes draw
/// bytes straight into guest RAM through this.
fn fresh_log() -> LogWriter {
    LogWriter::new(SegmentHeader {
        base_snapshot_id: [0; 32],
        entropy_seed: [0; 32],
        machine_config_hash: [0; 32],
        clock_num: 1,
        clock_den: 1,
        encoder_fingerprint: 0,
    })
}

fn config() -> MachineConfig {
    MachineConfig::new(
        MEM,
        [0x44; 32],
        BootSpec::Elf {
            kernel_hash: [0x44; 32],
            cmdline: Vec::new(),
        },
    )
}

fn read_count(mem: &GuestMemoryMmap<()>) -> u64 {
    let mut head = [0u8; 8];
    mem.read_slice(&mut head, GuestAddress(nanokernel::ENTROPY_DRAW_TABLE_GPA))
        .unwrap();
    u64::from_le_bytes(head)
}

/// Draws [from, from+n) out of the ring (capacity is far larger than any
/// run here, so no wrap-eviction can have occurred).
fn read_draws(mem: &GuestMemoryMmap<()>, from: u64, n: u64) -> Vec<u8> {
    let mut out = vec![0u8; usize::try_from(n * nanokernel::ENTROPY_DRAW_BYTES).expect("fits")];
    for i in 0..n {
        let slot = (from + i) & (nanokernel::ENTROPY_DRAW_RING_CAPACITY - 1);
        let gpa = nanokernel::ENTROPY_DRAW_TABLE_GPA + 8 + slot * nanokernel::ENTROPY_DRAW_BYTES;
        let at = usize::try_from(i * nanokernel::ENTROPY_DRAW_BYTES).expect("fits");
        let to = at + nanokernel::ENTROPY_DRAW_BYTES as usize;
        mem.read_slice(&mut out[at..to], GuestAddress(gpa)).unwrap();
    }
    out
}

/// Run `batches` guest batches (256 draws each), one segment per batch,
/// servicing the entropy-doorbell MMIO loop. Every segment must stop on
/// GuestHalted — the exact batch boundary.
#[allow(clippy::too_many_arguments)]
fn run_batches(
    slot: &mut SlotVm,
    counter: &InstRetired,
    chain: &mut StateHashChain,
    cfg: &MachineConfig,
    bus: &mut MmioBus,
    entropy: &mut DetEntropy,
    log: &mut LogWriter,
    batches: u64,
) -> SegmentOutcome {
    let mut last = None;
    for _ in 0..batches {
        last = Some(run_one_batch(slot, counter, chain, cfg, bus, entropy, log));
    }
    last.expect("at least one batch")
}

#[allow(clippy::too_many_arguments)]
fn run_one_batch(
    slot: &mut SlotVm,
    counter: &InstRetired,
    chain: &mut StateHashChain,
    cfg: &MachineConfig,
    bus: &mut MmioBus,
    entropy: &mut DetEntropy,
    log: &mut LogWriter,
) -> SegmentOutcome {
    let mut dev_mem = VmMem(slot.guest_mem.clone());
    let mut irqs = Vec::new();
    let start = counter.read().unwrap();
    let pause = AtomicBool::new(false);
    let out = {
        let counter_ref = counter;
        let mut seg = Segment {
            slot,
            counter,
            chain,
            config: cfg,
            start_icount: start,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
            hash_device_sections: None,
        };
        let mut on_exit = |exit: VcpuExit| {
            let icount = counter_ref
                .read()
                .map_err(|e| BoundaryError::Exit(format!("counter read: {e:?}")))?;
            let mut ctx = DevCtx::new(icount, 0, log, &mut dev_mem, entropy, &mut irqs);
            match exit {
                VcpuExit::MmioRead(gpa, data) => bus
                    .read(gpa, data, &mut ctx)
                    .map_err(|e| BoundaryError::Exit(format!("bus read {gpa:#x}: {e:?}")))?,
                VcpuExit::MmioWrite(gpa, data) => bus
                    .write(gpa, data, &mut ctx)
                    .map_err(|e| BoundaryError::Exit(format!("bus write {gpa:#x}: {e:?}")))?,
                other => return Err(BoundaryError::Exit(format!("unexpected exit: {other:?}"))),
            }
            if let Some(e) = ctx.log_fault() {
                return Err(BoundaryError::Exit(format!("log fault: {e:?}")));
            }
            Ok(())
        };
        run_segment(
            &mut seg,
            Until::IcountBudget(HUGE_BUDGET),
            &mut || false,
            &mut on_exit,
        )
        .expect("segment")
    };
    assert_eq!(
        out.reason,
        StopReason::GuestHalted,
        "expected the exact batch-boundary HLT"
    );
    assert!(irqs.is_empty(), "undrained irq queue");
    out
}

#[test]
fn restored_machine_draws_the_next_1024_fills_bit_identically() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let cfg = config();

    // ── Leg A: boot, draw past the snapshot point ─────────────────────────
    let mut slot_a = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot_a, nanokernel::entropy_draw_elf(), b"").unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let mut bus_a = test_bus();
    let mut entropy_a = DetEntropy::from_seed([0x42; 32]);
    let mut log_a = fresh_log();
    let mut chain_a = StateHashChain::new(&[0x44; 32], &[0x44; 32]);

    let a1 = run_batches(
        &mut slot_a,
        &counter,
        &mut chain_a,
        &cfg,
        &mut bus_a,
        &mut entropy_a,
        &mut log_a,
        BATCHES_BEFORE,
    );
    let count_pause = read_count(&slot_a.guest_mem);
    // Batch arithmetic is exact (also trips on the guest's fault poison).
    assert_eq!(count_pause, BATCHES_BEFORE * nanokernel::ENTROPY_DRAW_BATCH);

    // Snapshot mid-stream (the live PRNG position travels in ENTR v2).
    let snap = take_snapshot(
        &slot_a,
        SlotState::Paused,
        &bus_a,
        &entropy_a,
        &cfg,
        BoundaryState {
            icount: a1.boundary.icount,
            vns: a1.vns,
            epoch_index: a1.boundary.icount / cfg.epoch_len,
            hash_chain: chain_a.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        &store,
    )
    .expect("take_snapshot mid-stream");

    // The un-snapshotted continuation: the next GOLDEN_DRAWS are the
    // reference bytes.
    run_batches(
        &mut slot_a,
        &counter,
        &mut chain_a,
        &cfg,
        &mut bus_a,
        &mut entropy_a,
        &mut log_a,
        BATCHES_GOLDEN,
    );
    let golden = read_draws(&slot_a.guest_mem, count_pause, GOLDEN_DRAWS);
    assert!(
        golden.iter().any(|b| *b != 0),
        "golden draws are all zero — the doorbell path did not run"
    );

    // ── Leg B: restore into a fresh slot, draw the same span ─────────────
    let mut slot_b = sys.create_slot_vm(MEM).unwrap();
    let mut bus_b = test_bus();
    let outcome = restore_snapshot(
        &slot_b,
        SlotState::Paused,
        &mut bus_b,
        &cfg,
        snap.snapshot_ref,
        None, // continuous counter axis — see the module doc
        None,
        &store,
    )
    .expect("restore mid-stream snapshot");
    assert_eq!(read_count(&slot_b.guest_mem), count_pause);

    let mut entropy_b = outcome.entropy;
    let mut log_b = fresh_log();
    let mut chain_b = outcome.chain;
    run_batches(
        &mut slot_b,
        &counter,
        &mut chain_b,
        &cfg,
        &mut bus_b,
        &mut entropy_b,
        &mut log_b,
        BATCHES_GOLDEN,
    );

    // ── The acceptance ────────────────────────────────────────────────────
    let replayed = read_draws(&slot_b.guest_mem, count_pause, GOLDEN_DRAWS);
    assert_eq!(
        replayed, golden,
        "restored machine's next {GOLDEN_DRAWS} draws diverged from the \
         un-snapshotted continuation"
    );
    // The PRNGs ended at the same stream position too — the tuple round
    // trip is exact, not just prefix-equal.
    assert_eq!(entropy_b.state(), entropy_a.state());
    // And both legs drew the same exact total.
    assert_eq!(read_count(&slot_b.guest_mem), read_count(&slot_a.guest_mem));
    assert_eq!(
        read_count(&slot_b.guest_mem),
        (BATCHES_BEFORE + BATCHES_GOLDEN) * nanokernel::ENTROPY_DRAW_BATCH
    );
}
