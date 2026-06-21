//! Slot-manager live seams (bead ol1): the pieces the host-runnable
//! state-machine unit tests cannot prove —
//!
//! 1. `reset_slot_dirty_tracking`: a previously-RUN slot's stale dirty
//!    ring drains and resets, discharging restore_engine's FRESH-slot
//!    precondition for the same-slot reuse path the manager owns.
//! 2. `dh_vmm::run::pin_current_thread`: the affinity syscall actually
//!    confines the calling thread to the requested core (read back via
//!    sched_getaffinity), and the SCHED_FIFO promotion either succeeds
//!    (CAP_SYS_NICE, the lab-box deployment) or fails EPERM (dev boxes)
//!    — never anything else.
//!
//! HARDWARE-GATED where KVM is involved; the pinning checks run on any
//! Linux x86_64 host.

#![cfg(target_arch = "x86_64")]

mod common;

use common::kvm_available;
use dh_vmm::dirty::enable_dirty_logging;
use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem};
use dh_worker::slot_manager::reset_slot_dirty_tracking;

const MEM: u64 = 16 << 20;

/// Run page_dirtier to its halt so the dirty ring holds real entries,
/// then prove the reuse reset drains them all and leaves the ring empty.
#[test]
fn dirty_tracking_reset_drains_a_used_slot() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot, nanokernel::page_dirtier_elf(), b"").unwrap();
    enable_dirty_logging(&slot).expect("logging on");

    // The default 65536-entry ring swallows page_dirtier's 3072 pages
    // without a ring-full exit; the guest parks in HLT.
    match classify_exit(slot.vcpu.run().unwrap()) {
        ExitEvent::Hlt => {}
        other => panic!("unexpected exit {other:?}"),
    }

    // The slot is now exactly what restore_engine's precondition warns
    // about: Paused-looking but stale. The manager's reset must discard
    // every entry the run left behind...
    let stale = reset_slot_dirty_tracking(&slot).expect("first reset");
    assert!(
        stale >= nanokernel::PAGE_DIRTIER_PAGES as u32,
        "expected at least the {} guest-dirtied pages in the stale ring, drained {stale}",
        nanokernel::PAGE_DIRTIER_PAGES
    );

    // ...and a second reset proves the ring is genuinely empty (nothing
    // re-armed, nothing left): the slot is FRESH again as far as dirty
    // tracking is concerned.
    assert_eq!(
        reset_slot_dirty_tracking(&slot).expect("second reset"),
        0,
        "ring must be empty after the reuse reset"
    );
}

/// The affinity syscall confines the calling thread to the requested
/// core; FIFO promotion is privilege-dependent but never fails any
/// other way. Run in a scoped thread so the test harness thread's own
/// affinity is untouched.
#[test]
fn pin_current_thread_confines_and_fifo_is_privilege_gated() {
    use dh_vmm::run::{pin_current_thread, set_current_thread_fifo, PinError};

    let handle = std::thread::spawn(|| {
        // Core 1 (housekeeping): always inside the default cpuset, never
        // one of the isolated §7.4 slot cores another test could be
        // exercising under load.
        pin_current_thread(1).expect("affinity to core 1");

        // Read back: exactly core 1.
        // SAFETY: cpu_set_t is plain data; sched_getaffinity(0, ..)
        // fills it for the calling thread.
        #[allow(unsafe_code)]
        let confined = unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            assert_eq!(
                libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set),
                0
            );
            let on_1 = libc::CPU_ISSET(1, &set);
            let count = libc::CPU_COUNT(&set);
            on_1 && count == 1
        };
        assert!(confined, "thread must be confined to exactly core 1");

        // FIFO promotion: Ok with CAP_SYS_NICE (lab box), EPERM without
        // — anything else is a real bug.
        match set_current_thread_fifo() {
            Ok(()) => eprintln!("SCHED_FIFO granted (privileged host)"),
            Err(PinError::Scheduler(errno)) => {
                assert_eq!(errno, libc::EPERM, "only EPERM is an acceptable refusal");
                eprintln!("SCHED_FIFO refused with EPERM (unprivileged host) — tolerated");
            }
            Err(other) => panic!("unexpected pin failure: {other:?}"),
        }
    });
    handle.join().unwrap();
}
