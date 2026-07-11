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
//!
//! RUN-LOOP CONTRACT (spurious kicks): EINTR is a STOP REQUEST, not a
//! boundary assertion. Queued RT signals can deliver after a clear and
//! EINTR a later, legitimate KVM_RUN with the counter short of any target.
//! The boundary engine must always re-read the counter after EINTR and
//! decide — never assume EINTR means "period reached".
//!
//! Empirics (lab box, 40 runs, periods 100k/10k/1k/100): PMI overshoot is
//! exactly 18 instructions, zero variance, period-independent — pure
//! delivery latency, comfortably inside the default skid margin.
//! (That 18 is the single-instruction `jmp $` spin floor; the dh-cli
//! `skid` harness measures 27..31 on the landing loop, whose imul-led
//! dependent chain holds more in-flight retirement at NMI delivery —
//! same mechanism, workload-dependent constant, both ≪ margin/2.)

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
            // SAFETY: KickGuard<'a> holds the &mut VcpuFd borrow for its
            // whole life, so the pointer cannot dangle, and the handler
            // runs on the same thread (F_OWNER_TID routing). KNOWN ALIASING
            // CAVEAT: this reconstructs &mut while the interrupted run loop
            // holds one — benign (same thread, plain byte store into the
            // mmap'd kvm_run page; the firecracker-established pattern),
            // unavoidable until kvm-ioctls exposes the immediate_exit byte
            // address directly.
            #[allow(unsafe_code)]
            unsafe {
                (*ptr).set_kvm_immediate_exit(1);
            }
        }
    });
}

/// RAII registration of a vCPU as this thread's kick target.
///
/// The lifetime parameter holds the `&mut VcpuFd` borrow for the guard's
/// whole life — the compiler rejects moving or re-borrowing the VcpuFd
/// while registered, so the handler's raw pointer cannot dangle.
///
/// Registering also PRE-TOUCHES the TLS slot (the `with` call), which is
/// what makes the const-initialized thread_local access in the handler
/// allocation-free: by the time any signal can route here, the slot exists.
pub struct KickGuard<'a> {
    vcpu: &'a mut VcpuFd,
}

impl<'a> KickGuard<'a> {
    pub fn register(vcpu: &'a mut VcpuFd) -> Self {
        KICK_TARGET.with(|t| t.set(vcpu as *mut VcpuFd as usize));
        KickGuard { vcpu }
    }
}

/// All vCPU access while registered goes THROUGH the guard (Deref) — the
/// guard owns the exclusive borrow, so the VcpuFd cannot move or be
/// re-borrowed behind the handler's back.
impl std::ops::Deref for KickGuard<'_> {
    type Target = VcpuFd;
    fn deref(&self) -> &VcpuFd {
        self.vcpu
    }
}

impl std::ops::DerefMut for KickGuard<'_> {
    fn deref_mut(&mut self) -> &mut VcpuFd {
        self.vcpu
    }
}

impl Drop for KickGuard<'_> {
    fn drop(&mut self) {
        KICK_TARGET.with(|t| t.set(0));
    }
}

/// The calling thread's OS tid — what `route_overflow_to_thread` needs.
/// Lives here so unsafe-free callers (dh-cli) never reach for the pid,
/// which equals the tid ONLY on the main thread (a worker-thread caller
/// would silently route its PMI kicks to the wrong thread).
pub fn current_tid() -> i32 {
    // SAFETY: argless syscall.
    #[allow(unsafe_code)]
    unsafe {
        libc::syscall(libc::SYS_gettid) as i32
    }
}

/// Clear immediate_exit before re-entering KVM_RUN once the kick has been
/// consumed (the boundary engine calls this after handling the pause).
pub fn clear_immediate_exit(vcpu: &mut VcpuFd) {
    vcpu.set_kvm_immediate_exit(0);
}

// ── vCPU thread placement (ARCH §9, bead ol1) ───────────────────────────
//
// "1 vCPU thread per active slot (pinned, SCHED_FIFO prio 10 on its
// isolated core)". The slot→core MAP lives in dh-worker's slot manager;
// the SYSCALLS live here because dh-worker forbids unsafe code and
// thread plumbing is this module's home. Affinity and scheduling class
// are separate calls: SCHED_FIFO needs CAP_SYS_NICE, plain affinity
// does not — the daemon decides per deployment whether a FIFO EPERM is
// fatal.

/// ARCH §9 vCPU scheduling priority.
pub const VCPU_FIFO_PRIORITY: i32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinError {
    /// `sched_setaffinity` failed (errno) — e.g. the core is outside the
    /// process cpuset.
    Affinity(i32),
    /// `sched_setscheduler` failed (errno) — typically EPERM without
    /// CAP_SYS_NICE.
    Scheduler(i32),
}

/// Pin the CURRENT thread to `core` (the vCPU thread calls this with its
/// slot's dedicated isolated core before first guest entry).
pub fn pin_current_thread(core: u32) -> Result<(), PinError> {
    if core as usize >= libc::CPU_SETSIZE as usize {
        return Err(PinError::Affinity(libc::EINVAL));
    }
    // SAFETY: cpu_set_t is plain data, zero-initialized on the stack and
    // populated through libc's CPU_SET accessor; sched_setaffinity(0, ..)
    // targets the calling thread only.
    #[allow(unsafe_code)]
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_SET(core as usize, &mut set);
        if libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set) != 0 {
            return Err(PinError::Affinity(
                std::io::Error::last_os_error().raw_os_error().unwrap_or(-1),
            ));
        }
    }
    Ok(())
}

/// Promote the CURRENT thread to SCHED_FIFO at [`VCPU_FIFO_PRIORITY`].
pub fn set_current_thread_fifo() -> Result<(), PinError> {
    // SAFETY: sched_param is plain data; sched_setscheduler(0, ..)
    // targets the calling thread only.
    #[allow(unsafe_code)]
    unsafe {
        let param = libc::sched_param {
            sched_priority: VCPU_FIFO_PRIORITY,
        };
        if libc::sched_setscheduler(0, libc::SCHED_FIFO, &param) != 0 {
            return Err(PinError::Scheduler(
                std::io::Error::last_os_error().raw_os_error().unwrap_or(-1),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kvm::KvmSystem;
    use dh_detclock::counter::InstRetired;
    use vm_memory::{Bytes, GuestAddress};

    fn kvm_available() -> bool {
        crate::kvm::kvm_usable()
    }

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    #[test]
    fn pin_current_thread_rejects_cpu_set_overflow() {
        let first_invalid = u32::try_from(libc::CPU_SETSIZE)
            .unwrap_or(u32::MAX)
            .min(u32::MAX - 1);
        for core in [first_invalid, first_invalid + 1] {
            assert_eq!(
                pin_current_thread(core),
                Err(PinError::Affinity(libc::EINVAL))
            );
        }
    }

    /// The full §3.1 kick chain, live: guest spins forever; the armed
    /// counter overflows; the PMI signal kicks KVM_RUN out with EINTR; the
    /// counter reads ≥ period (overshoot = skid, bounded loosely here —
    /// the margin discipline is the boundary-engine bead).
    #[test]
    fn pmi_kick_interrupts_spinning_guest() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
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

        let mut guard = KickGuard::register(&mut slot.vcpu);
        let run_result = guard.run();
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
        clear_immediate_exit(&mut guard);
    }

    /// The no-lost-wakeup half AND the spurious-kick contract in one:
    /// a signal BEFORE KVM_RUN yields immediate EINTR without executing a
    /// single guest instruction — i.e. EINTR with the counter at 0, far
    /// from any target. This is exactly why the run loop must re-read the
    /// counter on EINTR rather than treat it as a boundary.
    #[test]
    fn kick_before_run_returns_immediately() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
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

        let mut guard = KickGuard::register(&mut slot.vcpu);
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
        let err = guard
            .run()
            .expect_err("immediate_exit must abort the entry");
        assert_eq!(err.errno(), libc::EINTR);
        clear_immediate_exit(&mut guard);
    }
}
