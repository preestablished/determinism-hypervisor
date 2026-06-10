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
        // Overflow fields (sample_period, wakeup_events) are armed per run
        // segment by the boundary engine bead, not at open time.
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
        if n != buf.len() {
            return Err(CounterError::Read(format!("short read: {n}")));
        }
        let value = u64::from_ne_bytes(buf[0..8].try_into().unwrap());
        let enabled = u64::from_ne_bytes(buf[8..16].try_into().unwrap());
        let running = u64::from_ne_bytes(buf[16..24].try_into().unwrap());
        if enabled > 0 && running == 0 {
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
        // The counter opens only where perf + a free PMU counter exist;
        // skip elsewhere (aarch64 dev boxes, CI containers).
        std::path::Path::new("/proc/sys/kernel/perf_event_paranoid").exists()
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
