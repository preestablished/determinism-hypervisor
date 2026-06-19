//! Guest-TSC alignment mechanisms (bead 3np; ARCH §4 defense-4 caveat).
//!
//! The §4.4 restore rule writes guest TSC ← vns. Two ways exist:
//!
//! 1. **Per-entry MSR value writes** (`KVM_SET_MSRS{IA32_TSC}`): simple,
//!    but engages KVM's TSC-sync heuristics (writes landing within the
//!    kernel's matching window can be QUANTIZED to an existing sync
//!    generation — a silent value perturbation), and at the §10 target of
//!    ~3k exits/guest-second a per-entry write adds a measurable ioctl
//!    tax.
//! 2. **The `KVM_VCPU_TSC_OFFSET` vCPU device attribute**: sets the exact
//!    hardware TSC offset once; no heuristics, no per-entry cost.
//!
//! The benchmark in this module's tests produces the measured numbers on
//! the lab box; the DECISION and the numbers are recorded in
//! docs/decisions/tsc-alignment.md. kvm-ioctls 0.24 cfg-gates its
//! device-attr wrappers to aarch64 (upstream gap — the ioctls are valid
//! on x86), so this module issues them raw, msr.rs-style.

use std::os::fd::AsRawFd;

use kvm_bindings::{kvm_device_attr, kvm_msr_entry, Msrs, KVM_VCPU_TSC_CTRL, KVM_VCPU_TSC_OFFSET};
use kvm_ioctls::VcpuFd;
use vmm_sys_util::ioctl::ioctl_with_ref;
use vmm_sys_util::ioctl_iow_nr;

use crate::kvm::KvmError;

const MSR_IA32_TSC: u32 = 0x10;

// include/uapi/linux/kvm.h: device-attr ioctls are valid on vCPU fds for
// the TSC control group.
ioctl_iow_nr!(KVM_SET_DEVICE_ATTR, 0xAE, 0xe1, kvm_device_attr);
// NB: GET is also _IOW in the kernel uapi (the kernel WRITES through
// attr.addr; the struct itself only travels userspace->kernel).
ioctl_iow_nr!(KVM_GET_DEVICE_ATTR, 0xAE, 0xe2, kvm_device_attr);
ioctl_iow_nr!(KVM_HAS_DEVICE_ATTR, 0xAE, 0xe3, kvm_device_attr);

/// `addr` carries the userspace pointer the kernel reads or WRITES
/// through — call sites derive it with the correct mutability (a *mut
/// for GET: the kernel's write target must never be born from a shared
/// reference, or release builds constant-fold the never-visibly-assigned
/// local to 0 — live-proven in review).
fn attr_for(addr: u64) -> kvm_device_attr {
    kvm_device_attr {
        flags: 0,
        group: KVM_VCPU_TSC_CTRL,
        attr: u64::from(KVM_VCPU_TSC_OFFSET),
        addr,
    }
}

/// Does this kernel expose the per-vCPU TSC offset attribute?
pub fn has_tsc_offset_attr(vcpu: &VcpuFd) -> bool {
    let attr = attr_for(0);
    // SAFETY: valid vCPU fd; HAS_DEVICE_ATTR only inspects group/attr.
    #[allow(unsafe_code)]
    let rc = unsafe { ioctl_with_ref(&vcpu.as_raw_fd(), KVM_HAS_DEVICE_ATTR(), &attr) };
    rc == 0
}

/// Mechanism 2: set the exact guest-TSC hardware offset (guest_tsc =
/// host_tsc + offset). One ioctl, no sync heuristics — the CHOSEN restore
/// mechanism (docs/decisions/tsc-alignment.md).
pub fn set_tsc_offset(vcpu: &VcpuFd, offset: i64) -> Result<(), KvmError> {
    let raw = offset as u64;
    let attr = attr_for(&raw as *const u64 as u64);
    // SAFETY: valid vCPU fd; the kernel reads the u64 through `addr`
    // during the ioctl; `raw` outlives the call.
    #[allow(unsafe_code)]
    let rc = unsafe { ioctl_with_ref(&vcpu.as_raw_fd(), KVM_SET_DEVICE_ATTR(), &attr) };
    if rc != 0 {
        return Err(KvmError::Open(format!(
            "KVM_SET_DEVICE_ATTR(TSC_OFFSET): {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

/// Set the current guest-visible TSC value by rebasing the hardware TSC
/// offset against the host TSC observed immediately before the ioctl.
pub fn set_tsc_value_with_offset(vcpu: &VcpuFd, value: u64) -> Result<(), KvmError> {
    let host_tsc = rdtsc();
    set_tsc_offset(vcpu, value.wrapping_sub(host_tsc) as i64)
}

/// Read the current guest-TSC offset back (verification + diagnostics).
pub fn get_tsc_offset(vcpu: &VcpuFd) -> Result<i64, KvmError> {
    let mut raw = 0u64;
    let attr = attr_for(std::ptr::addr_of_mut!(raw) as u64);
    // SAFETY: valid vCPU fd; the kernel writes the u64 through `addr`,
    // whose provenance is a *mut — never a shared reference.
    #[allow(unsafe_code)]
    let rc = unsafe { ioctl_with_ref(&vcpu.as_raw_fd(), KVM_GET_DEVICE_ATTR(), &attr) };
    if rc != 0 {
        return Err(KvmError::Open(format!(
            "KVM_GET_DEVICE_ATTR(TSC_OFFSET): {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(raw as i64)
}

fn rdtsc() -> u64 {
    // Safe intrinsic on x86_64.
    #[allow(unsafe_code)]
    unsafe {
        core::arch::x86_64::_rdtsc()
    }
}

/// Mechanism 1: write an absolute guest-TSC VALUE via KVM_SET_MSRS —
/// benchmarked for the record; NOT the chosen mechanism (sync-heuristic
/// hazard, per-entry cost).
pub fn set_tsc_value_msr(vcpu: &VcpuFd, value: u64) -> Result<(), KvmError> {
    let msrs = Msrs::from_entries(&[kvm_msr_entry {
        index: MSR_IA32_TSC,
        data: value,
        ..Default::default()
    }])
    .map_err(|e| KvmError::Open(format!("msr alloc: {e:?}")))?;
    let n = vcpu
        .set_msrs(&msrs)
        .map_err(|e| KvmError::Open(format!("KVM_SET_MSRS(IA32_TSC): {e}")))?;
    if n != 1 {
        return Err(KvmError::Open("KVM_SET_MSRS wrote 0 entries".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kvm::KvmSystem;

    /// Functional + benchmark, live: both mechanisms work; the offset
    /// attribute round-trips exactly; measured per-call costs go to
    /// stderr (and the decision doc).
    #[test]
    fn tsc_mechanisms_work_and_benchmark_live() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 << 20).unwrap();

        assert!(
            has_tsc_offset_attr(&slot.vcpu),
            "§2.1 requires KVM_CAP_VCPU_ATTRIBUTES; the TSC offset attr \
             must exist on the lab kernel"
        );

        // Exact round-trip: the offset is the value we set, bit for bit —
        // no heuristic interference.
        set_tsc_offset(&slot.vcpu, -123_456_789).unwrap();
        assert_eq!(get_tsc_offset(&slot.vcpu).unwrap(), -123_456_789);
        set_tsc_offset(&slot.vcpu, 42).unwrap();
        assert_eq!(get_tsc_offset(&slot.vcpu).unwrap(), 42);

        // MSR value write functions (no readable exactness guarantee —
        // the value is rebased onto the running host TSC).
        set_tsc_value_msr(&slot.vcpu, 1_000_000).unwrap();

        // Benchmark: 10k calls each (host tooling; std::time is fine
        // off the execution path).
        const N: u32 = 10_000;
        let t0 = std::time::Instant::now();
        for i in 0..N {
            set_tsc_offset(&slot.vcpu, i64::from(i)).unwrap();
        }
        let attr_ns = t0.elapsed().as_nanos() / u128::from(N);
        let t1 = std::time::Instant::now();
        for i in 0..N {
            set_tsc_value_msr(&slot.vcpu, u64::from(i)).unwrap();
        }
        let msr_ns = t1.elapsed().as_nanos() / u128::from(N);
        eprintln!(
            "TSC alignment benchmark: offset-attr {attr_ns} ns/call, \
             msr-write {msr_ns} ns/call (N={N})"
        );
        eprintln!(
            "at 3k exits/guest-s, per-entry MSR writes would cost \
             {} us/guest-s; the offset attribute is set ONCE per restore",
            (msr_ns * 3000) / 1000
        );
    }
}
