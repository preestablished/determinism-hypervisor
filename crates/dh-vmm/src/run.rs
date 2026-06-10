//! PMI → immediate-exit kick path (ARCH §3.1).
//!
//! The detclock counter's overflow signal must stop the vCPU race-free:
//! - signal lands DURING KVM_RUN → the ioctl returns EINTR (handler
//!   installed without SA_RESTART);
//! - signal lands OUTSIDE KVM_RUN → the handler sets
//!   `kvm_run.immediate_exit = 1`, so the NEXT KVM_RUN returns EINTR
//!   immediately. No lost wakeups (KVM_CAP_IMMEDIATE_EXIT protocol).
//!
//! The handler does exactly one thing — the immediate-exit store — through
//! a thread-local registration. Everything here runs on the vCPU thread.

use kvm_ioctls::VcpuFd;
use std::cell::Cell;

/// The chosen RT signal (§3.1: SIGRTMIN+4). Computed at runtime — glibc
/// reserves the first few RT signals.
pub fn kick_signal() -> i32 {
    libc::SIGRTMIN() + 4
}

thread_local! {
    /// The vCPU registered for kicks on THIS thread (raw pointer: the
    /// handler is async-signal context; Cell<usize> keeps it Copy + safe).
    static KICK_TARGET: Cell<usize> = const { Cell::new(0) };
}

/// Install the kick handler process-wide (idempotent; call once per
/// process before any vCPU registers). No SA_RESTART: KVM_RUN must EINTR.
pub fn install_kick_handler() -> Result<(), String> {
    // SAFETY: sigaction with a valid handler fn and zeroed mask; the
    // handler itself only performs an async-signal-safe store.
    #[allow(unsafe_code)]
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = kick_handler as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO;
        libc::sigemptyset(&mut sa.sa_mask);
        if libc::sigaction(kick_signal(), &sa, std::ptr::null_mut()) != 0 {
            return Err(format!("sigaction: {}", std::io::Error::last_os_error()));
        }
    }
    Ok(())
}

extern "C" fn kick_handler(
    _sig: libc::c_int,
    _info: *mut libc::siginfo_t,
    _ctx: *mut libc::c_void,
) {
    // Exactly one thing (§3.1): immediate_exit = 1 on the registered vCPU.
    // set_kvm_immediate_exit is a plain store into the mmap'd kvm_run page
    // — async-signal-safe.
    KICK_TARGET.with(|t| {
        let ptr = t.get() as *mut VcpuFd;
        if !ptr.is_null() {
            // SAFETY: registered by this thread via KickGuard, which keeps
            // the VcpuFd alive and clears the registration on drop; the
            // handler runs on the same thread (F_OWNER_TID routing).
            #[allow(unsafe_code)]
            unsafe {
                (*ptr).set_kvm_immediate_exit(1);
            }
        }
    });
}

/// RAII registration of a vCPU as this thread's kick target. Hold it for
/// the duration of the run loop; drop clears the registration before the
/// VcpuFd can move or die.
pub struct KickGuard;

impl KickGuard {
    pub fn register(vcpu: &mut VcpuFd) -> Self {
        KICK_TARGET.with(|t| t.set(vcpu as *mut VcpuFd as usize));
        KickGuard
    }
}

impl Drop for KickGuard {
    fn drop(&mut self) {
        KICK_TARGET.with(|t| t.set(0));
    }
}

/// Clear immediate_exit before re-entering KVM_RUN once the kick has been
/// consumed (the boundary engine calls this after handling the pause).
pub fn clear_immediate_exit(vcpu: &mut VcpuFd) {
    vcpu.set_kvm_immediate_exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kvm::KvmSystem;
    use dh_detclock::counter::InstRetired;
    use vm_memory::{Bytes, GuestAddress};

    fn kvm_available() -> bool {
        std::path::Path::new("/dev/kvm").exists()
    }

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    /// The full §3.1 kick chain, live: guest spins forever; the armed
    /// counter overflows; the PMI signal kicks KVM_RUN out with EINTR; the
    /// counter reads ≥ period (overshoot = skid, bounded loosely here —
    /// the margin discipline is the boundary-engine bead).
    #[test]
    fn pmi_kick_interrupts_spinning_guest() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        // Real-mode: jmp $ (EB FE) at 0x0.
        slot.guest_mem
            .write_slice(&[0xEB, 0xFE], GuestAddress(0))
            .unwrap();
        let mut sregs = slot.vcpu.get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        slot.vcpu.set_sregs(&sregs).unwrap();
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rip = 0;
        regs.rflags = 2;
        slot.vcpu.set_regs(&regs).unwrap();

        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), kick_signal())
            .unwrap();
        const PERIOD: u64 = 100_000;
        counter.arm_period(PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();

        let _guard = KickGuard::register(&mut slot.vcpu);
        let run_result = slot.vcpu.run();
        counter.disable().unwrap();

        // EINTR, whether the signal landed inside KVM_RUN or via the
        // immediate-exit path.
        let err = run_result.expect_err("spinning guest must be kicked out");
        assert_eq!(err.errno(), libc::EINTR, "kick must surface as EINTR");

        let count = counter.read().unwrap();
        assert!(
            count >= PERIOD,
            "counter ({count}) must have reached the armed period ({PERIOD})"
        );
        assert!(
            count < PERIOD + 50_000,
            "overshoot ({}) wildly exceeds plausible skid",
            count - PERIOD
        );
        clear_immediate_exit(&mut slot.vcpu);
    }

    /// The no-lost-wakeup half: signal BEFORE KVM_RUN → immediate EINTR
    /// without executing any guest instruction.
    #[test]
    fn kick_before_run_returns_immediately() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        slot.guest_mem
            .write_slice(&[0xEB, 0xFE], GuestAddress(0))
            .unwrap();
        let mut sregs = slot.vcpu.get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        slot.vcpu.set_sregs(&sregs).unwrap();
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rip = 0;
        regs.rflags = 2;
        slot.vcpu.set_regs(&regs).unwrap();

        let _guard = KickGuard::register(&mut slot.vcpu);
        // Deliver the kick signal to THIS thread now (outside KVM_RUN):
        // the handler must set immediate_exit so the run below cannot hang.
        // SAFETY: raising a handled signal on ourselves.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(
                libc::SYS_tgkill,
                libc::getpid(),
                libc::syscall(libc::SYS_gettid),
                kick_signal(),
            );
        }
        let err = slot
            .vcpu
            .run()
            .expect_err("immediate_exit must abort the entry");
        assert_eq!(err.errno(), libc::EINTR);
        clear_immediate_exit(&mut slot.vcpu);
    }
}
