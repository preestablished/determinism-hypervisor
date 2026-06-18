//! M9 Linux bzImage entry smoke. This is ignored by default because it needs
//! externally supplied Linux artifacts and live KVM.

#![cfg(target_arch = "x86_64")]

#[allow(dead_code)]
mod common;

use dh_vmm::config::canonicalize_bzimage_cmdline_extras;
use dh_vmm::kvm::KvmSystem;
use kvm_bindings::KVM_MAX_CPUID_ENTRIES;
use kvm_ioctls::VcpuExit;
use vm_memory::{Bytes, GuestAddress};

const M9_LINUX_MEM_BYTES: u64 = 512 * 1024 * 1024;
const LINUX_ENTRY_OFFSET: u64 = 0x200;

#[test]
#[ignore]
fn linux_entry_smoke() {
    if !common::kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }

    let artifacts = common::M9LinuxArtifacts::from_env_required("linux_entry_smoke")
        .expect("M9 Linux artifacts");
    let bzimage = std::fs::read(&artifacts.bzimage).expect("read DH_M9_BZIMAGE");
    let initramfs = std::fs::read(&artifacts.initramfs).expect("read DH_M9_INITRAMFS");
    let cmdline = canonicalize_bzimage_cmdline_extras(b"quiet").expect("canonical cmdline");

    let sys = KvmSystem::open().expect("KVM gate");
    let mut slot = sys
        .create_slot_vm(M9_LINUX_MEM_BYTES)
        .expect("create Linux entry slot");
    let layout = dh_vmm::boot::load_bzimage_and_enter(&slot, &bzimage, &initramfs, &cmdline)
        .expect("load bzImage and program entry");

    let regs = slot.vcpu.get_regs().expect("entry regs");
    assert_eq!(
        regs.rip,
        dh_vmm::boot::linux_bzimage::LINUX_KERNEL_LOAD_GPA + LINUX_ENTRY_OFFSET
    );
    assert_eq!(regs.rsi, dh_vmm::boot::linux_bzimage::LINUX_BOOT_PARAMS_GPA);
    assert_eq!(regs.rflags, 2);

    let sregs = slot.vcpu.get_sregs().expect("entry sregs");
    assert_eq!(sregs.cs.selector, 0x10, "Linux __BOOT_CS");
    assert_eq!(sregs.ds.selector, 0x18, "Linux __BOOT_DS");
    assert_ne!(sregs.cr0 & (1 << 31), 0, "paging enabled");
    assert_ne!(sregs.cr0 & 1, 0, "protected mode enabled");
    assert_ne!(sregs.cr4 & (1 << 5), 0, "PAE enabled");
    assert_ne!(sregs.efer & (1 << 8), 0, "LME enabled");
    assert_ne!(sregs.efer & (1 << 10), 0, "LMA enabled");

    let mut boot_params_magic = [0u8; 4];
    slot.guest_mem
        .read_slice(
            &mut boot_params_magic,
            GuestAddress(dh_vmm::boot::linux_bzimage::LINUX_BOOT_PARAMS_GPA + 0x202),
        )
        .expect("read boot_params magic");
    assert_eq!(&boot_params_magic, b"HdrS");
    assert_eq!(
        layout.kernel_image.start,
        dh_vmm::boot::linux_bzimage::LINUX_KERNEL_LOAD_GPA
    );

    assert_masked_cpuid_surface(&slot);

    match slot.vcpu.run().expect("first Linux KVM_RUN") {
        VcpuExit::Shutdown | VcpuExit::InternalError | VcpuExit::FailEntry(..) => {
            panic!("Linux entry failed before the first serviceable KVM exit")
        }
        exit => eprintln!("first Linux KVM exit after entry: {exit:?}"),
    }
}

fn assert_masked_cpuid_surface(slot: &dh_vmm::kvm::SlotVm) {
    let cpuid = slot
        .vcpu
        .get_cpuid2(KVM_MAX_CPUID_ENTRIES)
        .expect("get vcpu cpuid");
    assert!(
        !cpuid
            .as_slice()
            .iter()
            .any(|e| (0x4000_0000..0x4000_0100).contains(&e.function)),
        "KVM paravirt leaves, including kvmclock, must not be exposed"
    );

    for entry in cpuid.as_slice() {
        match (entry.function, entry.index) {
            (1, _) => {
                assert_eq!(entry.ecx & (1 << 21), 0, "x2APIC");
                assert_eq!(entry.ecx & (1 << 24), 0, "TSC-deadline");
                assert_eq!(entry.ecx & (1 << 30), 0, "RDRAND");
            }
            (7, 0) => assert_eq!(entry.ebx & (1 << 18), 0, "RDSEED"),
            (0x8000_0001, _) => assert_eq!(entry.edx & (1 << 27), 0, "RDTSCP"),
            (0x8000_0007, _) => assert_eq!(entry.edx & (1 << 8), 0, "invariant TSC"),
            _ => {}
        }
    }
}
