//! THE determinism regression (bead p9g, IMPLEMENTATION-PLAN M3 accept):
//! run the landing-loop guest for 1e9 instructions TWICE from cold boot
//! with fixed seeds — the full outcome, state-hash chain included, must
//! be bit-identical. Any mismatch is a P0 (MAP.md conventions).
//!
//! HARDWARE-GATED: runs in the kvm-intel lane (and on the lab box);
//! hosted lanes skip on the /dev/kvm probe.

// Everything here drives KVM (x86_64-only; bead v5w) - the whole test
// target compiles to empty on other arches.
#![cfg(target_arch = "x86_64")]

use std::io::ErrorKind;
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::KvmSystem;
use dh_vmm::runctl::{run_segment, Segment, StopReason, Until};
use kvm_ioctls::VcpuExit;

const BILLION: u64 = 1_000_000_000;
/// 1e9 instructions / 8 per landing-loop iteration.
const ITERS_CMDLINE: &[u8] = b"125000000";

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
    // Main-thread tid == pid is NOT guaranteed in tests (they run on
    // worker threads), so route via the real syscall-free interface:
    // std exposes no gettid; tests run single rig per thread, and the
    // counter must route to THIS thread.
    // SAFETY: argless syscall.
    #[allow(unsafe_code)]
    unsafe {
        libc::syscall(libc::SYS_gettid) as i32
    }
}

/// One cold-boot run to exactly `budget` instructions; full outcome.
fn cold_run(budget: u64) -> (u64, u64, u64, u64, [u8; 32]) {
    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(16 << 20).unwrap();
    dh_vmm::boot::load_and_enter(&slot, nanokernel::landing_loop_elf(), ITERS_CMDLINE).unwrap();

    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let config = MachineConfig::new(
        16 << 20,
        [7; 32], // fixed seed material: identical across the two runs
        BootSpec::Elf {
            kernel_hash: [7; 32],
            cmdline: ITERS_CMDLINE.to_vec(),
        },
    );
    let mut chain = StateHashChain::new(&[7; 32], &[7; 32]);
    let pause = AtomicBool::new(false);
    let mut seg = Segment {
        slot: &mut slot,
        counter: &counter,
        chain: &mut chain,
        config: &config,
        start_icount: 0,
        injections: &[],
        timer: None,
        pause: &pause,
        sdk_events: None,
    };
    let out = run_segment(
        &mut seg,
        Until::IcountBudget(budget),
        &mut || false,
        &mut |exit: VcpuExit| Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}"))),
    )
    .unwrap();
    assert_eq!(out.reason, StopReason::BudgetReached);
    assert_eq!(out.boundary.icount, budget, "landed exactly");
    (
        out.boundary.icount,
        out.boundary.rip,
        out.boundary.rcx,
        out.vns,
        out.state_hash,
    )
}

/// The M3 gate: 1e9 instructions, twice, equal everything.
/// With the default 50M epoch grid this also compares 20 intermediate
/// epoch hash links folded into the chain — not just the endpoint.
#[test]
fn one_billion_instructions_twice_equal_final_hash() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let a = cold_run(BILLION);
    let b = cold_run(BILLION);
    assert_eq!(
        a, b,
        "DETERMINISM REGRESSION (P0): two cold-boot 1e9 runs diverged"
    );
}

/// Fast smoke variant for quick local iteration (same shape, 1e7).
#[test]
fn ten_million_twice_equal_final_hash() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let a = cold_run(10_000_000);
    let b = cold_run(10_000_000);
    assert_eq!(a, b);
}
