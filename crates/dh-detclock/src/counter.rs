//! Guest-only pinned INST_RETIRED counter (ARCH §3.1).
//!
//! One perf_event_open counter per slot, attached to the vCPU thread:
//! PERF_COUNT_HW_INSTRUCTIONS, pinned (startup fails if the PMU can't grant
//! a dedicated counter — §7.4 requires nmi_watchdog=0 precisely so one is
//! free), exclude_host/hv/idle so ONLY guest-mode retired instructions
//! count, guest user+kernel both counted.
//!
//! Raw perf-event-open-sys, no high-level perf crate: we need guest-only
//! filtering plus (in the M2 boundary bead) signal-driven overflow via
//! F_SETOWN_EX/F_SETSIG → immediate_exit. This module owns open/read/
//! reset/enable; overflow arming is the boundary engine's bead.
//!
//! FALLBACK (risk R2, IMPLEMENTATION-PLAN): if INST_RETIRED's
//! interrupt-retirement empirics fail on this CPU/microcode class, the
//! documented alternative is retired conditional branches
//! (BR_INST_RETIRED.COND / .NEAR_TAKEN) with the (count, RIP, RCX)
//! boundary tuple — the swap is contained in this crate plus a
//! determinism-class bump.

use perf_event_open_sys as sys;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};

/// Sentinel arming period: large enough to never fire within a segment
/// (hard cap is 10e9 instructions), small enough to stay valid for the PMU.
pub const NEVER_FIRES_PERIOD: u64 = 1 << 62;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CounterError {
    /// perf_event_open failed (errno + context). EACCES usually means
    /// perf_event_paranoid is too strict (§7.4 sets it to 1).
    Open(String),
    /// The kernel granted the event but could not PIN it (counter stolen —
    /// NMI watchdog on, or PMU oversubscribed). Pinned-and-broken shows up
    /// as time_enabled > 0 with time_running == 0.
    NotPinned,
    Read(String),
    Ioctl(String),
}

/// The slot's instruction counter. Field order = drop order is irrelevant
/// here (single fd), but the fd closes on drop via File.
pub struct InstRetired {
    fd: File,
}

impl InstRetired {
    /// Open for the CALLING thread (the vCPU thread calls this on itself;
    /// pid=0, cpu=-1 follows the thread across the §7.4-pinned core).
    /// Starts disabled; the run loop enables around guest entry.
    pub fn open_for_current_thread() -> Result<Self, CounterError> {
        let mut attr = sys::bindings::perf_event_attr {
            type_: sys::bindings::PERF_TYPE_HARDWARE,
            size: std::mem::size_of::<sys::bindings::perf_event_attr>() as u32,
            config: u64::from(sys::bindings::PERF_COUNT_HW_INSTRUCTIONS),
            ..Default::default()
        };
        attr.set_pinned(1);
        attr.set_exclude_host(1);
        attr.set_exclude_hv(1);
        attr.set_exclude_idle(1);
        // exclude_user / exclude_kernel stay 0: guest user+kernel count.
        attr.set_disabled(1);
        // Open as a SAMPLING event (kernel rejects IOC_PERIOD on
        // non-sampling events) with a never-fires sentinel period; the
        // boundary engine re-arms the real period per run segment.
        attr.__bindgen_anon_1.sample_period = NEVER_FIRES_PERIOD;
        attr.__bindgen_anon_2.wakeup_events = 1; // §3.1
        attr.read_format = u64::from(
            sys::bindings::PERF_FORMAT_TOTAL_TIME_ENABLED
                | sys::bindings::PERF_FORMAT_TOTAL_TIME_RUNNING,
        );

        // SAFETY: perf_event_open with a valid attr pointer; the returned
        // fd is immediately owned by File.
        #[allow(unsafe_code)]
        let fd = unsafe { sys::perf_event_open(&mut attr, 0, -1, -1, 0) };
        if fd < 0 {
            return Err(CounterError::Open(format!(
                "perf_event_open: {}",
                std::io::Error::last_os_error()
            )));
        }
        #[allow(unsafe_code)]
        let fd = unsafe { File::from_raw_fd(fd) };
        Ok(Self { fd })
    }

    /// Counter value. Reads happen only while the vCPU is out of guest
    /// mode (§3.1), so the value is stable. Verifies the pinned contract on
    /// every read: enabled-but-not-running means the PMU revoked us.
    pub fn read(&self) -> Result<u64, CounterError> {
        // read_format: value, time_enabled, time_running.
        let mut buf = [0u8; 24];
        let n = nix_read(&self.fd, &mut buf)?;
        if n == 0 {
            // Empirics (Linux 6.8, lab box): a pinned event that lost the
            // PMU returns a ZERO-byte read — this, not the time_running
            // signature below, is the real revocation path. Verified by
            // oversubscribing the PMU with 31 pinned counters (22 dead,
            // all read 0 bytes).
            return Err(CounterError::NotPinned);
        }
        if n != buf.len() {
            return Err(CounterError::Read(format!("short read: {n}")));
        }
        let value = u64::from_ne_bytes(buf[0..8].try_into().unwrap());
        let enabled = u64::from_ne_bytes(buf[8..16].try_into().unwrap());
        let running = u64::from_ne_bytes(buf[16..24].try_into().unwrap());
        if enabled > 0 && running == 0 {
            // Belt-and-braces: kernel-version-dependent secondary signature
            // (the primary one on 6.8 is the 0-byte read above). Sched-in is
            // synchronous on enable(), so this cannot false-fire right
            // after enable (verified empirically).
            return Err(CounterError::NotPinned);
        }
        Ok(value)
    }

    /// Re-zero at every restore/fork (§3.1): icount is segment-relative,
    /// so the latch is always 0.
    pub fn reset(&self) -> Result<(), CounterError> {
        // SAFETY: valid perf fd; argless perf ioctls.
        #[allow(unsafe_code)]
        let rc = unsafe { sys::ioctls::RESET(self.fd.as_raw_fd(), 0) };
        ioctl_result("RESET", rc)
    }

    pub fn enable(&self) -> Result<(), CounterError> {
        // SAFETY: as above.
        #[allow(unsafe_code)]
        let rc = unsafe { sys::ioctls::ENABLE(self.fd.as_raw_fd(), 0) };
        ioctl_result("ENABLE", rc)
    }

    /// Arm the overflow period for the next run segment (§3.1
    /// PERF_EVENT_IOC_PERIOD). The boundary engine sets period =
    /// (target icount - current icount) - skid_margin.
    pub fn arm_period(&self, period: u64) -> Result<(), CounterError> {
        let mut p = period;
        // SAFETY: valid perf fd; PERF_EVENT_IOC_PERIOD reads a u64 through
        // the pointer passed as the ioctl arg (the sys wrapper forwards the
        // u64 verbatim, so we pass the address).
        #[allow(unsafe_code)]
        let rc = unsafe { sys::ioctls::PERIOD(self.fd.as_raw_fd(), &mut p as *mut u64 as u64) };
        ioctl_result("PERIOD", rc)
    }

    /// Route overflow signals to the vCPU thread (§3.1): F_SETOWN_EX with
    /// F_OWNER_TID + F_SETSIG(signo) + O_ASYNC. The signal handler (dh-vmm
    /// run module) sets kvm_run.immediate_exit — the KVM_CAP_IMMEDIATE_EXIT
    /// protocol makes the kick race-free.
    pub fn route_overflow_to_thread(&self, tid: i32, signo: i32) -> Result<(), CounterError> {
        // libc lacks f_owner_ex on this target; define the ABI struct
        // locally (kernel: include/uapi/asm-generic/fcntl.h).
        #[repr(C)]
        struct FOwnerEx {
            type_: libc::c_int,
            pid: libc::pid_t,
        }
        const F_OWNER_TID: libc::c_int = 0;
        const F_SETOWN_EX: libc::c_int = 15;
        const F_SETSIG: libc::c_int = 10;
        let owner = FOwnerEx {
            type_: F_OWNER_TID,
            pid: tid,
        };
        // SAFETY: valid fd; fcntl with a valid f_owner_ex pointer / flags.
        #[allow(unsafe_code)]
        unsafe {
            if libc::fcntl(self.fd.as_raw_fd(), F_SETOWN_EX, &owner) != 0 {
                return Err(CounterError::Ioctl(format!(
                    "F_SETOWN_EX: {}",
                    std::io::Error::last_os_error()
                )));
            }
            if libc::fcntl(self.fd.as_raw_fd(), F_SETSIG, signo) != 0 {
                return Err(CounterError::Ioctl(format!(
                    "F_SETSIG: {}",
                    std::io::Error::last_os_error()
                )));
            }
            let flags = libc::fcntl(self.fd.as_raw_fd(), libc::F_GETFL);
            if flags < 0
                || libc::fcntl(self.fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_ASYNC) != 0
            {
                return Err(CounterError::Ioctl(format!(
                    "O_ASYNC: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }
        Ok(())
    }

    pub fn disable(&self) -> Result<(), CounterError> {
        // SAFETY: as above.
        #[allow(unsafe_code)]
        let rc = unsafe { sys::ioctls::DISABLE(self.fd.as_raw_fd(), 0) };
        ioctl_result("DISABLE", rc)
    }
}

fn ioctl_result(name: &str, rc: libc::c_int) -> Result<(), CounterError> {
    if rc != 0 {
        return Err(CounterError::Ioctl(format!(
            "perf ioctl {name}: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

fn nix_read(fd: &File, buf: &mut [u8]) -> Result<usize, CounterError> {
    // SAFETY: plain read into a stack buffer of the stated length.
    #[allow(unsafe_code)]
    let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
    if n < 0 {
        return Err(CounterError::Read(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Ok(n as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pmu_available() -> bool {
        // The counter opens only where perf exists AND policy lets this user
        // open a guest-filtered counter. Existence of the sysctl file is not
        // enough: GitHub-hosted runners have it but set paranoid=4, which
        // denies every unprivileged perf_event_open (EACCES). §7.4 provisions
        // the lab box at paranoid=1, so anything stricter means "skip", and
        // a failure to open at <=1 stays a real test failure.
        let Ok(s) = std::fs::read_to_string("/proc/sys/kernel/perf_event_paranoid") else {
            return false;
        };
        let level: i32 = s.trim().parse().unwrap_or(i32::MAX);
        // SAFETY: geteuid has no preconditions and cannot fail.
        #[allow(unsafe_code)]
        let euid = unsafe { libc::geteuid() };
        level <= 1 || euid == 0
    }

    #[test]
    fn opens_pinned_guest_only_counter() {
        if !pmu_available() {
            eprintln!("skipping: no perf");
            return;
        }
        let c = InstRetired::open_for_current_thread().expect("§7.4 host must grant a counter");
        // Guest-only: with no guest running, enable+work+read counts 0.
        c.reset().unwrap();
        c.enable().unwrap();
        let mut x = 0u64;
        for i in 0..100_000u64 {
            x = x.wrapping_add(i * 31);
        }
        std::hint::black_box(x);
        c.disable().unwrap();
        let host_work_count = c.read().unwrap();
        assert_eq!(
            host_work_count, 0,
            "exclude_host must keep host instructions invisible"
        );
    }

    #[test]
    fn reset_rezeroes() {
        if !pmu_available() {
            eprintln!("skipping: no perf");
            return;
        }
        let c = InstRetired::open_for_current_thread().unwrap();
        c.reset().unwrap();
        assert_eq!(c.read().unwrap(), 0);
    }
}
