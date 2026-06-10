//! Default-deny MSR filter + deterministic MSR emulation (ARCH §2.2).
//!
//! KVM_X86_SET_MSR_FILTER with default-deny for read AND write; allow
//! ranges only for MSRs the guest legitimately uses. Everything else exits
//! KVM_EXIT_X86_RDMSR/WRMSR (USER_SPACE_MSR + EXIT_REASON_FILTER, enabled
//! per-VM in kvm.rs) and is emulated deterministically:
//! - unknown READ → a fixed config value (0), trace-logged by the caller;
//! - unknown WRITE → guest #GP (kvm_run.msr.error = 1). Surfacing beats
//!   silently absorbing nondeterminism (risk R6).
//!
//! Allow-list note: the bead names "EFER, STAR/LSTAR/CSTAR/FMASK, FS/GS
//! base, SYSENTER_*, PAT, TSC_AUX". KERNEL_GS_BASE is additionally allowed:
//! it is on the §8.1 capture list (which is "complete by construction" —
//! an MSR the guest wrote that was NOT allowed would have faulted at write
//! time), and every 64-bit kernel writes it on context switch (swapgs).

use kvm_bindings::{
    kvm_msr_filter, kvm_msr_filter_range, KVM_MSR_FILTER_DEFAULT_DENY, KVM_MSR_FILTER_READ,
    KVM_MSR_FILTER_WRITE,
};
use kvm_ioctls::VmFd;
use std::os::fd::AsRawFd;
use vmm_sys_util::ioctl::ioctl_with_ref;
use vmm_sys_util::ioctl_iow_nr;

// Allowed architectural MSRs (Intel SDM numbers).
pub const MSR_SPEC_CTRL: u32 = 0x48;
pub const MSR_SYSENTER_CS: u32 = 0x174;
pub const MSR_SYSENTER_ESP: u32 = 0x175;
pub const MSR_SYSENTER_EIP: u32 = 0x176;
pub const MSR_PAT: u32 = 0x277;
pub const MSR_EFER: u32 = 0xC000_0080;
pub const MSR_STAR: u32 = 0xC000_0081;
pub const MSR_LSTAR: u32 = 0xC000_0082;
pub const MSR_CSTAR: u32 = 0xC000_0083;
pub const MSR_SFMASK: u32 = 0xC000_0084;
pub const MSR_FS_BASE: u32 = 0xC000_0100;
pub const MSR_GS_BASE: u32 = 0xC000_0101;
pub const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102;
pub const MSR_TSC_AUX: u32 = 0xC000_0103;

/// The allow ranges: [base, base+count). Contiguous where the numbering
/// permits; every other MSR is default-denied.
const ALLOW_RANGES: &[(u32, u32)] = &[
    (MSR_SPEC_CTRL, 1), // SPEC_CTRL: §8.1-captured guest state (reset 0, no
    // host leak — live-verified); Linux writes it at
    // early boot, so denying breaks the M7 guest.
    (MSR_SYSENTER_CS, 3), // SYSENTER_CS/ESP/EIP
    (MSR_PAT, 1),         // PAT
    (MSR_EFER, 5),        // EFER, STAR, LSTAR, CSTAR, SFMASK
    (MSR_FS_BASE, 4),     // FS_BASE, GS_BASE, KERNEL_GS_BASE, TSC_AUX
];
// DELIBERATELY DENIED despite §8.1 capture-list membership: IA32_TSC (0x10).
// It is genuinely host-derived (live: 0x19ebc vs 0x18702 on two fresh VMs —
// the canonical R6 leak) and §4.4 writes it host-side on restore; guest
// reads go through pv-clock, guest writes #GP by design.

ioctl_iow_nr!(KVM_X86_SET_MSR_FILTER, 0xAE, 0xc6, kvm_msr_filter);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsrFilterError(pub String);

/// Apply the default-deny filter to a VM. Call once after create_slot_vm
/// (USER_SPACE_MSR is already enabled there with EXIT_REASON_FILTER).
pub fn apply_default_deny_filter(vm: &VmFd) -> Result<(), MsrFilterError> {
    // Each range needs a bitmap: one bit per MSR, 1 = ALLOWED (with
    // DEFAULT_DENY, listed ranges are consulted; set bits pass). Bitmaps
    // must outlive the ioctl only — kernel copies them in.
    let mut bitmaps: Vec<Vec<u8>> = ALLOW_RANGES
        .iter()
        .map(|(_, count)| vec![0xFFu8; (*count as usize).div_ceil(8)])
        .collect();

    let mut filter = kvm_msr_filter {
        flags: KVM_MSR_FILTER_DEFAULT_DENY,
        ..Default::default()
    };
    for (i, ((base, count), bitmap)) in ALLOW_RANGES.iter().zip(bitmaps.iter_mut()).enumerate() {
        filter.ranges[i] = kvm_msr_filter_range {
            flags: KVM_MSR_FILTER_READ | KVM_MSR_FILTER_WRITE,
            nmsrs: *count,
            base: *base,
            bitmap: bitmap.as_mut_ptr(),
        };
    }

    // SAFETY: valid VM fd; filter and all bitmap pointers are live across
    // the call; the kernel copies the structures during the ioctl.
    #[allow(unsafe_code)]
    let rc = unsafe { ioctl_with_ref(&vm.as_raw_fd(), KVM_X86_SET_MSR_FILTER(), &filter) };
    if rc != 0 {
        return Err(MsrFilterError(format!(
            "KVM_X86_SET_MSR_FILTER: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

/// Deterministic emulation policy for filter-denied accesses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsrAction {
    /// RDMSR: supply this fixed value (and trace-log the index upstream).
    SupplyValue(u64),
    /// WRMSR: inject #GP (set kvm_run.msr.error = 1).
    InjectGp,
}

/// Unknown READ → fixed value 0 for every denied MSR. One fixed table,
/// part of machine behavior — never the hardware value (risk R6).
pub fn on_denied_rdmsr(_index: u32) -> MsrAction {
    MsrAction::SupplyValue(0)
}

/// Unknown WRITE → #GP, always. Surfacing beats absorbing.
pub fn on_denied_wrmsr(_index: u32) -> MsrAction {
    MsrAction::InjectGp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kvm::KvmSystem;
    use vm_memory::{Bytes, GuestAddress};

    fn kvm_available() -> bool {
        std::path::Path::new("/dev/kvm").exists()
    }

    #[test]
    fn policy_is_fixed_and_total() {
        assert_eq!(on_denied_rdmsr(0xE7), MsrAction::SupplyValue(0));
        assert_eq!(on_denied_rdmsr(0x1A0), MsrAction::SupplyValue(0));
        assert_eq!(on_denied_wrmsr(0xE7), MsrAction::InjectGp);
    }

    #[test]
    fn allow_ranges_cover_exactly_the_chosen_set() {
        let allowed: Vec<u32> = ALLOW_RANGES
            .iter()
            .flat_map(|(base, count)| *base..*base + *count)
            .collect();
        assert_eq!(
            allowed,
            vec![
                MSR_SPEC_CTRL,
                MSR_SYSENTER_CS,
                MSR_SYSENTER_ESP,
                MSR_SYSENTER_EIP,
                MSR_PAT,
                MSR_EFER,
                MSR_STAR,
                MSR_LSTAR,
                MSR_CSTAR,
                MSR_SFMASK,
                MSR_FS_BASE,
                MSR_GS_BASE,
                MSR_KERNEL_GS_BASE,
                MSR_TSC_AUX,
            ]
        );
    }

    /// Live: denied RDMSR exits to userspace with REASON_FILTER (not #GP),
    /// carrying the index — the deterministic emulation entry point.
    #[test]
    fn denied_rdmsr_exits_to_userspace() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        apply_default_deny_filter(&slot.vm).unwrap();

        // Real mode: mov ecx, 0xE7 (MPERF — denied); rdmsr; hlt
        slot.guest_mem
            .write_slice(
                &[0x66, 0xB9, 0xE7, 0x00, 0x00, 0x00, 0x0F, 0x32, 0xF4],
                GuestAddress(0),
            )
            .unwrap();
        let mut sregs = slot.vcpu.get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        slot.vcpu.set_sregs(&sregs).unwrap();
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rip = 0;
        regs.rflags = 2;
        slot.vcpu.set_regs(&regs).unwrap();

        // Route through the WIRED dispatch path (classify_exit applies the
        // deterministic policy: data=0, error=0).
        match crate::kvm::classify_exit(slot.vcpu.run().unwrap()) {
            crate::kvm::ExitEvent::MsrReadDenied { index } => assert_eq!(index, 0xE7),
            other => panic!("expected MsrReadDenied, got {other:?}"),
        }
        let exit = slot.vcpu.run().unwrap();
        assert_eq!(format!("{exit:?}"), "Hlt");
        // The supplied value is the policy value: EDX:EAX == 0.
        let regs = slot.vcpu.get_regs().unwrap();
        assert_eq!(regs.rax & 0xFFFF_FFFF, 0);
        assert_eq!(regs.rdx & 0xFFFF_FFFF, 0);
    }

    /// Live: denied WRMSR through the wired path injects #GP — the guest
    /// never reaches HLT (real mode, no IDT ⇒ the #GP escalates to a
    /// shutdown/triple-fault exit). Without the error=1 wiring, KVM
    /// silently acks the write and the guest DOES reach HLT
    /// (live-verified during review — the R6 violation this test pins).
    #[test]
    fn denied_wrmsr_injects_gp() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        apply_default_deny_filter(&slot.vm).unwrap();

        // mov ecx, 0xE7; mov eax, 1; xor edx, edx; wrmsr; hlt
        slot.guest_mem
            .write_slice(
                &[
                    0x66, 0xB9, 0xE7, 0x00, 0x00, 0x00, // mov ecx, 0xE7
                    0x66, 0xB8, 0x01, 0x00, 0x00, 0x00, // mov eax, 1
                    0x66, 0x31, 0xD2, // xor edx, edx
                    0x0F, 0x30, // wrmsr
                    0xF4, // hlt
                ],
                GuestAddress(0),
            )
            .unwrap();
        let mut sregs = slot.vcpu.get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        slot.vcpu.set_sregs(&sregs).unwrap();
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rip = 0;
        regs.rflags = 2;
        slot.vcpu.set_regs(&regs).unwrap();

        match crate::kvm::classify_exit(slot.vcpu.run().unwrap()) {
            crate::kvm::ExitEvent::MsrWriteDenied { index, data } => {
                assert_eq!(index, 0xE7);
                assert_eq!(data, 1);
            }
            other => panic!("expected MsrWriteDenied, got {other:?}"),
        }
        // #GP armed: the guest must NOT reach HLT.
        let exit = slot.vcpu.run().unwrap();
        assert_ne!(
            format!("{exit:?}"),
            "Hlt",
            "denied WRMSR must fault, never silently succeed"
        );
    }

    /// Live: allowed MSR (EFER read) does NOT exit — KVM handles it.
    #[test]
    fn allowed_msr_does_not_exit() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        apply_default_deny_filter(&slot.vm).unwrap();

        // mov ecx, 0xC0000080 (EFER — allowed); rdmsr; hlt
        slot.guest_mem
            .write_slice(
                &[0x66, 0xB9, 0x80, 0x00, 0x00, 0xC0, 0x0F, 0x32, 0xF4],
                GuestAddress(0),
            )
            .unwrap();
        let mut sregs = slot.vcpu.get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        slot.vcpu.set_sregs(&sregs).unwrap();
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rip = 0;
        regs.rflags = 2;
        slot.vcpu.set_regs(&regs).unwrap();

        let exit = slot.vcpu.run().unwrap();
        assert_eq!(format!("{exit:?}"), "Hlt", "EFER read must not exit");
    }
}
