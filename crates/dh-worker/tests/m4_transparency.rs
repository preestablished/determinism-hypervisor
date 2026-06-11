//! M4 ACCEPT: snapshot transparency (bead 7c8; IMPLEMENTATION-PLAN M4
//! accept). Run the landing-loop guest 1e8 instructions, TakeSnapshot at
//! the boundary, DESTROY the slot, restore into a fresh slot from the
//! REAL snapshot-store, run 1e8 more → H1. Versus the same 2e8 with a
//! plain pause at 1e8 and no snapshot machinery → H2. H1 == H2 EXACTLY —
//! every epoch link in the §8.5 chain is a full-RAM walk plus the
//! canonical vCPU blob, so any instruction-count drift, device-state
//! leak, or RAM byte the restore failed to reproduce shows here.
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

use std::io::ErrorKind;
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::clock::PvClock;
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::pad::PvPad;
use dh_devices::serial::DebugSerial;
use dh_devices::MmioBus;
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::runctl::{run_segment, Segment, SegmentOutcome, StopReason, Until};
use dh_vmm::SlotState;
use dh_worker::restore_engine::restore_snapshot;
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use kvm_ioctls::VcpuExit;
use snapstore_client::blocking::SnapstoreClient as BlockingClient;
use snapstore_client::Transport;
use snapstore_server::build_server::{serve_for_tests, ServerHandle};
use snapstore_server::config::ServerConfig;
use tempfile::TempDir;

const MEM: u64 = 16 << 20;
const HALF: u64 = 100_000_000; // epoch grid (50M) point — both legs link here
const FULL: u64 = 200_000_000;
/// Landing-loop iterations: 8 instructions each; 30M iters = 2.4e8
/// capacity, so neither leg ever reaches the guest's completion HLT.
const ITERS_CMDLINE: &[u8] = b"30000000";

fn kvm_usable() -> bool {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
    {
        Ok(_) => true,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) => false,
        Err(e) => panic!("unexpected /dev/kvm probe failure: {e}"),
    }
}

fn gettid() -> i32 {
    // SAFETY: argless syscall.
    #[allow(unsafe_code)]
    unsafe {
        libc::syscall(libc::SYS_gettid) as i32
    }
}

fn spawn_store_blocking() -> (
    tokio::runtime::Runtime,
    ServerHandle,
    BlockingClient,
    TempDir,
) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let dir = TempDir::new().expect("tempdir");
    let data_root = dir.path().to_path_buf();
    let config = ServerConfig {
        data_root: data_root.clone(),
        grpc_tcp_addr: "127.0.0.1:0".parse().expect("addr"),
        grpc_uds_path: Some(data_root.join("snapstore.sock")),
        page_channel_path: None,
        http_addr: "127.0.0.1:0".parse().expect("addr"),
        pagestore: Default::default(),
        meta: Default::default(),
        page_channel: Default::default(),
    };
    let (handle, uds) = rt
        .block_on(serve_for_tests(config))
        .expect("serve_for_tests");
    let mut client = None;
    for _ in 0..50 {
        match BlockingClient::connect(Transport::Uds(uds.clone())) {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    (rt, handle, client.expect("store ready"), dir)
}

fn test_bus() -> MmioBus {
    let mut bus = MmioBus::new();
    bus.register(0xD000_1000, Box::new(PvPad::new())).unwrap();
    bus.register(0xD000_2000, Box::new(PvClock::new(1, 1)))
        .unwrap();
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    bus.register(0xD000_6000, Box::new(DebugSerial::new()))
        .unwrap();
    bus
}

fn config() -> MachineConfig {
    MachineConfig::new(
        MEM,
        [7; 32], // fixed seed material — identical across both legs
        BootSpec::Elf {
            kernel_hash: [7; 32],
            cmdline: ITERS_CMDLINE.to_vec(),
        },
    )
}

/// Cold-boot a landing-loop slot with its own counter, ready to run.
fn boot() -> (SlotVm, InstRetired, MachineConfig, StateHashChain) {
    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let slot = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot, nanokernel::landing_loop_elf(), ITERS_CMDLINE).unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();
    (
        slot,
        counter,
        config(),
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
) -> SegmentOutcome {
    let start = counter.read().unwrap();
    let pause = AtomicBool::new(false);
    let mut seg = Segment {
        slot,
        counter,
        chain,
        config,
        start_icount: start,
        injections: &[],
        timer: None,
        pause: &pause,
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

/// The milestone gate. One test, both legs, exact equality.
#[test]
fn snapshot_restore_roundtrip_is_invisible_to_the_hash_chain() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }

    // ── Control leg: 1e8 + pause + 1e8, no snapshot machinery ────────────
    let (mut slot, counter, cfg, mut chain) = boot();
    let c1 = run_more(&mut slot, &counter, &mut chain, &cfg, HALF);
    let c2 = run_more(&mut slot, &counter, &mut chain, &cfg, FULL - HALF);
    let h2 = c2.state_hash;
    drop(slot);

    // ── Roundtrip leg: 1e8 + TakeSnapshot + destroy + restore + 1e8 ──────
    let (mut slot, counter, cfg, mut chain) = boot();
    let r1 = run_more(&mut slot, &counter, &mut chain, &cfg, HALF);

    // Cold-boot determinism (the M3 property) must hold first — otherwise
    // any H1/H2 mismatch below would be ambiguous.
    assert_eq!(r1, c1, "the two legs diverged BEFORE the snapshot");

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

    // DESTROY: the original slot (VM fd, vCPU fd, RAM mapping) is gone.
    // The first leg's chain is dead too — shadow it so nothing below can
    // touch pre-snapshot state by accident.
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
    assert_eq!(outcome.cumulative_icount, HALF);
    assert_eq!(outcome.vns, r1.vns);

    // Second half on the restored slot, chain resumed from TIME.
    let mut chain = outcome.chain;
    let r2 = run_more(&mut slot, &counter, &mut chain, &cfg, FULL - HALF);

    // ── The milestone property ────────────────────────────────────────────
    let h1 = r2.state_hash;
    assert_eq!(r2.boundary, c2.boundary, "landing position diverged");
    assert_eq!(r2.vns, c2.vns, "virtual time diverged");
    assert_eq!(
        h1, h2,
        "H1 != H2: the snapshot/restore detour is VISIBLE in the state-hash \
         chain — an instruction-count, device-state, or RAM leak"
    );
}
