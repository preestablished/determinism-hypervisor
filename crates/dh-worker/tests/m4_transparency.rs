//! M4 ACCEPT: snapshot transparency (bead 7c8; IMPLEMENTATION-PLAN M4
//! accept). Run the landing-loop guest 1e8 instructions, TakeSnapshot at
//! the boundary, DESTROY the slot, restore into a fresh slot from the
//! REAL snapshot-store, run 1e8 more → H1. Versus the same 2e8 with a
//! plain pause at 1e8 and no snapshot machinery → H2, and an
//! UNINTERRUPTED single-segment 2e8 → H3. H3 == H2 proves the pause
//! itself is invisible; H1 == H2 proves the snapshot/destroy/restore
//! detour is.
//!
//! WHAT THE CHAIN GATES (scope honesty, iteration-75 review): every 50M
//! epoch link is a full-RAM walk + the canonical vCPU blob + (icount,
//! vns) — so instruction-count drift, any RAM byte the restore failed to
//! reproduce, and any non-TSC vCPU divergence all show. Two things are
//! OUTSIDE this gate by construction: (1) raw guest TSC — the canonical
//! blob normalizes the TSC slot to vns (§8.1), and the landing loop never
//! RDTSCs, so control's free-running TSC vs the restored leg's
//! vns-programmed TSC_OFFSET is invisible here (the TSC≡vns discipline is
//! tsc.rs's contract, pinned by its own tests); (2) device/bus state —
//! runctl links hash device_sections=&[] in Phase 1 and these segments
//! never touch MMIO, so device transparency is owned by the
//! restore_engine joint tests (byte-level section round-trip + identical
//! re-snapshot ref) and the ENTR golden bead (dy8).
//!
//! Counter axis note: runctl computes agendas and hash-link icounts in
//! COUNTER space, and the counter counts only guest instructions
//! (exclude_host) — so the restore leg keeps the SAME counter running
//! (restore_snapshot's `counter: None`), which both legs' chains share as
//! the cumulative axis. The §3.1 counter-reset-to-zero path (fresh worker,
//! cumulative base carried in TIME) is exercised in dh-worker
//! tests/restore_engine.rs; wiring the cumulative offset into runctl is
//! run-control scope (bead ol1). The `counter.read() == start_icount`
//! check inside run_segment doubles as proof that the whole
//! snapshot+destroy+restore detour executed ZERO guest instructions.
//!
//! PLACEMENT: the M4 plan names tests/determinism, but ARCH §1's
//! normative "nothing depends on dh-worker" rule (CI-enforced by
//! tests/arch_dependency_rule.rs) forbids that package from importing the
//! engines — so the acceptance lives with the engines instead.
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

// Everything here drives KVM (x86_64-only; bead v5w) — the whole test
// target compiles to empty on other arches.
#![cfg(target_arch = "x86_64")]

mod common;

use std::sync::atomic::AtomicBool;

use common::{kvm_available, spawn_store_blocking, test_bus};
use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::entropy::DetEntropy;
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::runctl::{run_segment, ScheduledInjection, Segment, SegmentOutcome, StopReason, Until};
use dh_vmm::SlotState;
use dh_worker::fork_engine::fork_slot;
use dh_worker::restore_engine::restore_snapshot;
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use kvm_ioctls::VcpuExit;
use vm_memory::{Bytes, GuestAddress};

const MEM: u64 = 16 << 20;
const HALF: u64 = 100_000_000; // epoch grid (50M) point — all legs link here
const FULL: u64 = 200_000_000;
/// Landing-loop iterations: 8 instructions each; 30M iters = 2.4e8
/// capacity, so no leg ever reaches the guest's completion HLT.
const ITERS_CMDLINE: &[u8] = b"30000000";

fn gettid() -> i32 {
    // SAFETY: argless syscall.
    #[allow(unsafe_code)]
    unsafe {
        libc::syscall(libc::SYS_gettid) as i32
    }
}

fn config(cmdline: &[u8]) -> MachineConfig {
    MachineConfig::new(
        MEM,
        [7; 32], // fixed seed material — identical across all legs
        BootSpec::Elf {
            kernel_hash: [7; 32],
            cmdline: cmdline.to_vec(),
        },
    )
}

/// Cold-boot a guest slot with its own counter, ready to run.
#[allow(clippy::type_complexity)]
fn boot(
    elf: &[u8],
    cmdline: &[u8],
) -> (
    KvmSystem,
    SlotVm,
    InstRetired,
    MachineConfig,
    StateHashChain,
) {
    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let slot = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot, elf, cmdline).unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();
    (
        sys,
        slot,
        counter,
        config(cmdline),
        StateHashChain::new(&[7; 32], &[7; 32]),
    )
}

/// One segment of `more` instructions on an already-positioned slot.
fn run_more(
    slot: &mut SlotVm,
    counter: &InstRetired,
    chain: &mut StateHashChain,
    config: &MachineConfig,
    more: u64,
    injections: &[ScheduledInjection],
) -> SegmentOutcome {
    let start = counter.read().unwrap();
    let pause = AtomicBool::new(false);
    let mut seg = Segment {
        slot,
        counter,
        chain,
        config,
        start_icount: start,
        injections,
        timer: None,
        pause: &pause,
        sdk_events: None,
    };
    let out = run_segment(
        &mut seg,
        Until::IcountBudget(more),
        &mut || false,
        &mut |exit: VcpuExit| Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}"))),
    )
    .unwrap();
    assert_eq!(out.reason, StopReason::BudgetReached);
    assert_eq!(out.boundary.icount, start + more, "landed exactly");
    out
}

/// The milestone gate. Three legs, exact equality.
#[test]
fn snapshot_restore_roundtrip_is_invisible_to_the_hash_chain() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }

    // ── Control leg: 1e8 + pause + 1e8, no snapshot machinery ────────────
    let (_sys, mut slot, counter, cfg, mut chain) =
        boot(nanokernel::landing_loop_elf(), ITERS_CMDLINE);
    let c1 = run_more(&mut slot, &counter, &mut chain, &cfg, HALF, &[]);
    let c2 = run_more(&mut slot, &counter, &mut chain, &cfg, FULL - HALF, &[]);
    let h2 = c2.state_hash;
    drop(slot);

    // ── Uninterrupted leg: one 2e8 segment, no pause at 1e8 at all. The
    //    epoch grid still links at 50/100/150/200M, so the chain is
    //    comparable — H3 == H2 pins that the segment SPLIT is invisible,
    //    isolating what H1 == H2 then attributes to the snapshot detour. ──
    let (_sys, mut slot, counter, cfg, mut chain) =
        boot(nanokernel::landing_loop_elf(), ITERS_CMDLINE);
    let u = run_more(&mut slot, &counter, &mut chain, &cfg, FULL, &[]);
    assert_eq!(
        u.state_hash, h2,
        "H3 != H2: pausing at 1e8 is itself visible — segment-split bug, \
         not a snapshot bug"
    );
    drop(slot);

    // ── Roundtrip leg: 1e8 + TakeSnapshot + destroy + restore + 1e8 ──────
    let (_sys, mut slot, counter, cfg, mut chain) =
        boot(nanokernel::landing_loop_elf(), ITERS_CMDLINE);
    let r1 = run_more(&mut slot, &counter, &mut chain, &cfg, HALF, &[]);

    // Cold-boot determinism (the M3 property) must hold first — otherwise
    // any H1/H2 mismatch below would be ambiguous.
    assert_eq!(r1, c1, "the legs diverged BEFORE the snapshot");

    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([9; 32]);
    let snap = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &cfg,
        BoundaryState {
            icount: r1.boundary.icount,
            vns: r1.vns,
            epoch_index: r1.boundary.icount / cfg.epoch_len,
            hash_chain: chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        &store,
    )
    .expect("take_snapshot at the 1e8 boundary");
    assert_eq!(snap.pages_shipped, MEM / 4096);

    // DESTROY: the original slot (VM fd, vCPU fd, RAM mapping) is gone,
    // and the first chain value lives on only inside the stored TIME
    // section (`let _` keeps the borrow checker from letting anything
    // below reuse it directly).
    drop(slot);
    drop(bus);
    let _ = chain;

    // Fresh slot, fresh bus, no boot — everything comes from the store.
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    let mut bus = test_bus();
    let outcome = restore_snapshot(
        &slot,
        SlotState::Paused,
        &mut bus,
        &cfg,
        snap.snapshot_ref,
        None, // keep the shared counter axis — see the module doc note
        None,
        &store,
    )
    .expect("restore into the fresh slot");
    assert_eq!(outcome.pages_loaded, MEM / 4096);
    assert_eq!(outcome.cumulative_icount, HALF);
    assert_eq!(outcome.vns, r1.vns);
    assert_eq!(outcome.epoch_index, HALF / cfg.epoch_len);
    // The chain came back through the store's TIME section and must
    // resume from EXACTLY the pre-destroy link value.
    assert_eq!(outcome.chain.value(), r1.state_hash);

    // Second half on the restored slot, chain resumed from TIME.
    let mut chain = outcome.chain;
    let r2 = run_more(&mut slot, &counter, &mut chain, &cfg, FULL - HALF, &[]);

    // ── The milestone property ────────────────────────────────────────────
    let h1 = r2.state_hash;
    assert_eq!(
        h1, h2,
        "H1 != H2: the snapshot/restore detour is VISIBLE in the state-hash \
         chain — an instruction-count, RAM, or (non-TSC) vCPU-state leak"
    );
    // Full outcome equality (reason, boundary tuple, vns, hash, delivery
    // counts) — anything the segment reports must match, not just the hash.
    assert_eq!(r2, c2, "segment outcomes diverged");
}

/// M4 ACCEPT leg 2 (bead a6s, first half): the same H1 == H2 property
/// with a TIER-A FORK in the middle instead of a store round-trip. The
/// parent freezes at 1e8; the child's RAM is a CoW view, its vCPU and
/// devices arrive through the in-memory DHSNAP — and the chain must not
/// be able to tell. Same counter-axis convention as the restore leg
/// (`counter: None`, see the module doc).
#[test]
fn fork_roundtrip_is_invisible_to_the_hash_chain() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }

    // Control: 1e8 + pause + 1e8 (identical to the restore leg's control).
    let (_sys, mut slot, counter, cfg, mut chain) =
        boot(nanokernel::landing_loop_elf(), ITERS_CMDLINE);
    let c1 = run_more(&mut slot, &counter, &mut chain, &cfg, HALF, &[]);
    let c2 = run_more(&mut slot, &counter, &mut chain, &cfg, FULL - HALF, &[]);
    drop(slot);

    // Fork leg: 1e8, freeze, fork, run the CHILD 1e8 more.
    let (sys, mut parent, counter, cfg, mut chain) =
        boot(nanokernel::landing_loop_elf(), ITERS_CMDLINE);
    let r1 = run_more(&mut parent, &counter, &mut chain, &cfg, HALF, &[]);
    assert_eq!(r1, c1, "the legs diverged BEFORE the fork");

    parent.freeze_ram().expect("freeze parent");
    let bus_p = test_bus();
    let entropy_p = DetEntropy::from_seed([9; 32]);
    let mut bus_c = test_bus();
    let outcome = fork_slot(
        &sys,
        &parent,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &cfg,
        BoundaryState {
            icount: r1.boundary.icount,
            vns: r1.vns,
            epoch_index: r1.boundary.icount / cfg.epoch_len,
            hash_chain: chain.value(),
            agenda_empty: true,
        },
        &mut bus_c,
        None, // keep the shared counter axis — see the module doc note
    )
    .expect("fork at the 1e8 boundary");
    assert_eq!(outcome.cumulative_icount, HALF);
    assert_eq!(outcome.chain.value(), r1.state_hash);

    let mut child = outcome.child;
    let mut chain = outcome.chain;
    let r2 = run_more(&mut child, &counter, &mut chain, &cfg, FULL - HALF, &[]);

    assert_eq!(
        r2.state_hash, c2.state_hash,
        "H1 != H2: the fork detour is VISIBLE in the state-hash chain"
    );
    assert_eq!(r2, c2, "segment outcomes diverged across the fork");
}

/// M4 ACCEPT leg 2 (bead a6s, second half): frozen-parent second-child
/// reproducibility. Child A runs with inputs X (scheduled vector
/// injections the timer guest's ISRs record); child B — forked AFTER A
/// ran and diverged — replays the same X and must match A EXACTLY
/// (chain value = full-RAM + vCPU walk, plus the guest-visible ISR
/// delivery table). Children run on the §3.1 reset counter axis
/// (`counter: Some` in the fork), so both share an identical 0-based
/// agenda for X — and the chain links' vns is pure counter-space too
/// (vt.rs has no vns_base term; PvClock's base only feeds guest MMIO
/// reads, which this guest never does), so A and B are comparable by
/// construction, not by accident.
#[test]
fn frozen_parent_children_replay_identical_inputs_identically() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    const PARENT_RUN: u64 = 2_000_000;
    const CHILD_RUN: u64 = 2_000_000;
    let inputs_x: Vec<ScheduledInjection> = vec![
        ScheduledInjection {
            icount: 500_000,
            vector: 0x40,
        },
        ScheduledInjection {
            icount: 1_000_000,
            vector: 0x41,
        },
        ScheduledInjection {
            icount: 1_500_000,
            vector: 0x40,
        },
    ];

    let (sys, mut parent, counter, cfg, mut chain) = boot(nanokernel::timer_guest_elf(), b"");
    let r_parent = run_more(&mut parent, &counter, &mut chain, &cfg, PARENT_RUN, &[]);
    parent.freeze_ram().expect("freeze parent");
    let bus_p = test_bus();
    let entropy_p = DetEntropy::from_seed([0x33; 32]);
    let fork_boundary = BoundaryState {
        icount: r_parent.boundary.icount,
        vns: r_parent.vns,
        epoch_index: r_parent.boundary.icount / cfg.epoch_len,
        hash_chain: chain.value(),
        agenda_empty: true,
    };

    let run_child = |label: &str, injections: &[ScheduledInjection]| {
        let mut bus_c = test_bus();
        let outcome = fork_slot(
            &sys,
            &parent,
            SlotState::Frozen,
            &bus_p,
            &entropy_p,
            &cfg,
            fork_boundary,
            &mut bus_c,
            Some(&counter), // §3.1: each child's segment counts from 0
        )
        .unwrap_or_else(|e| panic!("fork {label}: {e:?}"));
        let mut child = outcome.child;
        let mut chain = outcome.chain;
        assert_eq!(outcome.cumulative_icount, PARENT_RUN);
        let out = run_more(
            &mut child, &counter, &mut chain, &cfg, CHILD_RUN, injections,
        );

        // The guest-visible record of X: the timer guest's ISR table.
        let mut head = [0u8; 8];
        child
            .guest_mem
            .read_slice(&mut head, GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA))
            .unwrap();
        let count = u64::from_le_bytes(head);
        let mut vectors = vec![0u8; count as usize];
        child
            .guest_mem
            .read_slice(
                &mut vectors,
                GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA + 8),
            )
            .unwrap();
        (out, count, vectors)
    };

    let (out_a, count_a, vec_a) = run_child("A", &inputs_x);
    // Child A diverged (it ran with X); child B forks AFTER that and must
    // reproduce A bit-for-bit from the frozen base.
    let (out_b, count_b, vec_b) = run_child("B", &inputs_x);

    assert_eq!(out_a.injections_delivered, 3, "inputs X did not all land");
    assert_eq!(count_a, 3, "guest table disagrees with delivery count");
    assert_eq!(vec_a, vec![0x40, 0x41, 0x40], "guest saw the wrong vectors");
    assert_eq!(out_a, out_b, "second child diverged from the first");
    assert_eq!((count_a, &vec_a), (count_b, &vec_b));

    // Divergence sensitivity (the test CAN fail): a third child with
    // DIFFERENT inputs Y must produce a different hash and table.
    let inputs_y: Vec<ScheduledInjection> = vec![
        ScheduledInjection {
            icount: 600_000,
            vector: 0x41,
        },
        ScheduledInjection {
            icount: 1_100_000,
            vector: 0x40,
        },
    ];
    let (out_y, count_y, vec_y) = run_child("Y", &inputs_y);
    assert_eq!(out_y.injections_delivered, 2);
    assert_ne!(out_y.state_hash, out_a.state_hash, "X vs Y must diverge");
    assert_ne!((count_y, &vec_y), (count_a, &vec_a));

    // The frozen parent stayed pristine through three children: its ISR
    // table never recorded a delivery.
    let mut parent_head = [0u8; 8];
    parent
        .guest_mem
        .read_slice(
            &mut parent_head,
            GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA),
        )
        .unwrap();
    assert_eq!(
        u64::from_le_bytes(parent_head),
        0,
        "child deliveries leaked into the frozen parent's table"
    );
}
