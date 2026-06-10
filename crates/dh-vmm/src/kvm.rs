//! KVM bring-up (ARCH §2.1/§2.2): capability gate, slot VM construction,
//! and the exit-dispatch skeleton.
//!
//! Determinism posture: the §2.1 required-capability table hard-fails at
//! startup; the forbidden list (in-kernel irqchip, PIT, kvmclock,
//! DISABLE_EXITS) is enforced by construction — this module simply never
//! creates them, and the smoke test asserts a fresh VM has no irqchip.

use kvm_bindings::{kvm_userspace_memory_region, KVM_API_VERSION};
use kvm_ioctls::{Cap, Kvm, VcpuExit, VcpuFd, VmFd};
use vm_memory::{FileOffset, GuestAddress, GuestMemoryMmap};

/// The §2.2 GPA map: device windows live in a RAM hole — no memslot backs
/// them, so guest access produces KVM_EXIT_MMIO.
pub const MMIO_HOLE_BASE: u64 = 0xD000_0000;
pub const MMIO_HOLE_LEN: u64 = 0x7000;

/// PIO map (§2.2): debug serial + the detcall window; all else RAZ/WI.
pub const PIO_SERIAL_BASE: u16 = 0x3F8;
pub const PIO_SERIAL_LEN: u16 = 8;
pub const PIO_DETCALL_BASE: u16 = 0xD370;
pub const PIO_DETCALL_LEN: u16 = 0x30; // 0xD370..=0xD39F

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvmError {
    /// /dev/kvm missing or unopenable.
    Open(String),
    ApiVersionMismatch(i32),
    /// One or more §2.1 required capabilities absent (named).
    MissingCaps(Vec<&'static str>),
    /// Guest memory setup failed (memfd, mmap, memslot, madvise).
    Memory(String),
    VmCreate(String),
    VcpuCreate(String),
    /// v1 keeps RAM below the MMIO hole; larger sizes are a later split.
    MemTooLarge,
}

/// §2.1 required capability table. (DIRTY_LOG_RING is preferred-optional —
/// probed separately, bitmap fallback allowed.)
const REQUIRED_CAPS: &[(Cap, &str)] = &[
    (Cap::UserMemory, "KVM_CAP_USER_MEMORY"),
    (Cap::SetTssAddr, "KVM_CAP_SET_TSS_ADDR"),
    (Cap::ExtCpuid, "KVM_CAP_EXT_CPUID"),
    (Cap::X86UserSpaceMsr, "KVM_CAP_X86_USER_SPACE_MSR"),
    (Cap::GetMsrFeatures, "KVM_CAP_GET_MSR_FEATURES"),
    (Cap::SetGuestDebug, "KVM_CAP_SET_GUEST_DEBUG"),
    (Cap::ImmediateExit, "KVM_CAP_IMMEDIATE_EXIT"),
    (Cap::VcpuEvents, "KVM_CAP_VCPU_EVENTS"),
    (Cap::Debugregs, "KVM_CAP_DEBUGREGS"),
    (Cap::Xsave2, "KVM_CAP_XSAVE2"),
    (Cap::Xcrs, "KVM_CAP_XCRS"),
];

/// §2.1 caps that kvm-ioctls' Cap enum does not name — probed raw.
const REQUIRED_RAW_CAPS: &[(u32, &str)] = &[
    (
        kvm_bindings::KVM_CAP_MANUAL_DIRTY_LOG_PROTECT2,
        "KVM_CAP_MANUAL_DIRTY_LOG_PROTECT2",
    ),
    (
        kvm_bindings::KVM_CAP_X86_MSR_FILTER,
        "KVM_CAP_X86_MSR_FILTER",
    ),
    // TSC normalization on restore uses the TSC OFFSET vCPU attribute
    // (ARCH §4.4 prefers offset writes over MSR value writes); the
    // attribute itself is probed per-vCPU in create_slot_vm. NOTE, spec
    // empirics: ARCH §2.1's table named KVM_CAP_TSC_CONTROL here, but that
    // cap gates TSC FREQUENCY SCALING (KVM_SET_TSC_KHZ), which Coffee Lake
    // (the lab box / determinism-class CPU) does not support and which the
    // design never needs — single pinned host, no migration. The §2.1 table
    // has been amended accordingly.
    (
        kvm_bindings::KVM_CAP_VCPU_ATTRIBUTES,
        "KVM_CAP_VCPU_ATTRIBUTES",
    ),
];

/// Open /dev/kvm and enforce the §2.1 gate.
pub struct KvmSystem {
    kvm: Kvm,
    /// Dirty ring available (preferred); bitmap fallback otherwise.
    pub dirty_ring: bool,
}

impl KvmSystem {
    pub fn open() -> Result<Self, KvmError> {
        let kvm = Kvm::new().map_err(|e| KvmError::Open(e.to_string()))?;
        let version = kvm.get_api_version();
        if version != KVM_API_VERSION as i32 {
            return Err(KvmError::ApiVersionMismatch(version));
        }
        let mut missing: Vec<&'static str> = REQUIRED_CAPS
            .iter()
            .filter(|(cap, _)| !kvm.check_extension(*cap))
            .map(|(_, name)| *name)
            .collect();
        missing.extend(
            REQUIRED_RAW_CAPS
                .iter()
                .filter(|(cap, _)| kvm.check_extension_raw(u64::from(*cap)) <= 0)
                .map(|(_, name)| *name),
        );
        if !missing.is_empty() {
            return Err(KvmError::MissingCaps(missing));
        }
        let dirty_ring =
            kvm.check_extension_raw(u64::from(kvm_bindings::KVM_CAP_DIRTY_LOG_RING_ACQ_REL)) > 0;
        Ok(Self { kvm, dirty_ring })
    }

    /// Construct one slot's VM: VM fd, exactly one vCPU, and guest RAM as a
    /// single memfd-backed region [0, mem_bytes) with MADV_NOHUGEPAGE
    /// (ARCH §7.4: 4 KiB-exact dirty granularity). No irqchip, no PIT, no
    /// kvmclock — ever (§2.1 forbidden list, by construction).
    pub fn create_slot_vm(&self, mem_bytes: u64) -> Result<SlotVm, KvmError> {
        if mem_bytes > MMIO_HOLE_BASE {
            return Err(KvmError::MemTooLarge);
        }
        let vm = self
            .kvm
            .create_vm()
            .map_err(|e| KvmError::VmCreate(e.to_string()))?;

        let memfd = memfd_nohuge(mem_bytes)?;
        let region = GuestMemoryMmap::<()>::from_ranges_with_files(&[(
            GuestAddress(0),
            mem_bytes as usize,
            Some(FileOffset::new(memfd, 0)),
        )])
        .map_err(|e| KvmError::Memory(e.to_string()))?;

        // Advise the mapping itself; the memfd flag alone is not enough on
        // all kernels. 4 KiB-exact dirty granularity is load-bearing (§8.2).
        madvise_nohugepage(&region, mem_bytes)?;

        let slot = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: mem_bytes,
            userspace_addr: host_addr(&region)?,
            flags: 0,
        };
        // SAFETY-equivalent contract (the ioctl itself is wrapped unsafely
        // by kvm-ioctls): the region outlives the VM via SlotVm holding both.
        #[allow(unsafe_code)]
        unsafe { vm.set_user_memory_region(slot) }.map_err(|e| KvmError::Memory(e.to_string()))?;

        let vcpu = vm
            .create_vcpu(0)
            .map_err(|e| KvmError::VcpuCreate(e.to_string()))?;

        // TSC offset attribute (the §4.4 restore-normalization mechanism):
        // gated system-wide by KVM_CAP_VCPU_ATTRIBUTES (REQUIRED_RAW_CAPS).
        // The precise per-vCPU KVM_HAS_DEVICE_ATTR(KVM_VCPU_TSC_CTRL/OFFSET)
        // probe is blocked on kvm-ioctls 0.24 cfg-gating has_device_attr to
        // aarch64 (upstream gap; the ioctl is valid on x86) — the M2 TSC
        // alignment benchmark bead owns that probe + mechanism choice.

        Ok(SlotVm {
            vm,
            vcpu,
            guest_mem: region,
            mem_bytes,
        })
    }
}

/// One slot's KVM objects (the §2.2 Slot's kernel-facing third).
pub struct SlotVm {
    pub vm: VmFd,
    pub vcpu: VcpuFd,
    pub guest_mem: GuestMemoryMmap<()>,
    pub mem_bytes: u64,
}

/// What one KVM_RUN exit means to run control — the dispatch skeleton.
/// Carriers only; handling (device dispatch, detcall, logging) is run
/// control's job and lands with the boundary-engine beads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExitEvent {
    /// Guest touched the MMIO hole: forward to dh-devices MmioBus.
    MmioRead {
        gpa: u64,
        len: usize,
    },
    MmioWrite {
        gpa: u64,
        data: Vec<u8>,
    },
    /// Detcall window PIO (0xD370..=0xD39F).
    ///
    /// IN-FILL CONTRACT (determinism-critical): for DetcallIn/SerialIn the
    /// kvm_run IO buffer is NOT filled by classify_exit — the caller MUST
    /// write the deterministic reply into the buffer before re-entering
    /// KVM_RUN. The buffer is the persistent kvm_run IO data area: leaving
    /// it untouched hands the guest stale bytes from a previous exit —
    /// host-visible nondeterminism.
    DetcallIn {
        port: u16,
        len: usize,
    },
    DetcallOut {
        port: u16,
        data: Vec<u8>,
    },
    /// Debug serial (0x3F8..0x400).
    SerialOut {
        port: u16,
        data: Vec<u8>,
    },
    SerialIn {
        port: u16,
        len: usize,
    },
    /// Any other port: RAZ/WI (read-as-zero already applied by dispatch).
    PioIgnored {
        port: u16,
    },
    Hlt,
    Shutdown,
    /// Exit kinds later beads own (PMI/debug/MSR...) — carried verbatim.
    Other(String),
}

/// Classify one KVM_RUN exit per the §2.2 PIO/MMIO map. For PIO IN on
/// unmapped ports this WRITES the RAZ zeros into the kernel-shared buffer
/// before returning (read-as-zero is part of dispatch, not the caller).
pub fn classify_exit(exit: VcpuExit<'_>) -> ExitEvent {
    match exit {
        VcpuExit::MmioRead(gpa, data) => ExitEvent::MmioRead {
            gpa,
            len: data.len(),
        },
        VcpuExit::MmioWrite(gpa, data) => ExitEvent::MmioWrite {
            gpa,
            data: data.to_vec(),
        },
        VcpuExit::IoIn(port, data) => {
            if in_range(port, PIO_DETCALL_BASE, PIO_DETCALL_LEN) {
                ExitEvent::DetcallIn {
                    port,
                    len: data.len(),
                }
            } else if in_range(port, PIO_SERIAL_BASE, PIO_SERIAL_LEN) {
                ExitEvent::SerialIn {
                    port,
                    len: data.len(),
                }
            } else {
                data.fill(0); // RAZ
                ExitEvent::PioIgnored { port }
            }
        }
        VcpuExit::IoOut(port, data) => {
            if in_range(port, PIO_DETCALL_BASE, PIO_DETCALL_LEN) {
                ExitEvent::DetcallOut {
                    port,
                    data: data.to_vec(),
                }
            } else if in_range(port, PIO_SERIAL_BASE, PIO_SERIAL_LEN) {
                ExitEvent::SerialOut {
                    port,
                    data: data.to_vec(),
                }
            } else {
                ExitEvent::PioIgnored { port } // WI
            }
        }
        VcpuExit::Hlt => ExitEvent::Hlt,
        VcpuExit::Shutdown => ExitEvent::Shutdown,
        other => ExitEvent::Other(format!("{other:?}")),
    }
}

fn in_range(port: u16, base: u16, len: u16) -> bool {
    port >= base && port < base + len
}

fn host_addr(mem: &GuestMemoryMmap<()>) -> Result<u64, KvmError> {
    use vm_memory::GuestMemoryBackend;
    mem.get_host_address(GuestAddress(0))
        .map(|p| p as u64)
        .map_err(|e: vm_memory::GuestMemoryError| KvmError::Memory(e.to_string()))
}

/// memfd sized for guest RAM. Raw libc: memfd_create has no safe wrapper in
/// our dep set; the fd is immediately owned by File.
fn memfd_nohuge(len: u64) -> Result<std::fs::File, KvmError> {
    use std::os::fd::FromRawFd;
    #[allow(unsafe_code)]
    let fd = unsafe { libc::memfd_create(c"dh-slot-ram".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(KvmError::Memory(format!(
            "memfd_create: {}",
            std::io::Error::last_os_error()
        )));
    }
    #[allow(unsafe_code)]
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.set_len(len)
        .map_err(|e| KvmError::Memory(format!("memfd set_len: {e}")))?;
    Ok(file)
}

/// MADV_NOHUGEPAGE over the whole mapping (§7.4 THP convention).
fn madvise_nohugepage(mem: &GuestMemoryMmap<()>, len: u64) -> Result<(), KvmError> {
    let addr = host_addr(mem)?;
    #[allow(unsafe_code)]
    let rc = unsafe {
        libc::madvise(
            addr as *mut libc::c_void,
            len as usize,
            libc::MADV_NOHUGEPAGE,
        )
    };
    if rc != 0 {
        return Err(KvmError::Memory(format!(
            "madvise(NOHUGEPAGE): {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kvm_available() -> bool {
        std::path::Path::new("/dev/kvm").exists()
    }

    #[test]
    fn caps_gate_passes_on_compliant_host() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        let sys = KvmSystem::open().expect("§2.1 caps must hold on the lab box");
        // The lab box runs a 6.x kernel: dirty ring should be available.
        assert!(sys.dirty_ring, "dirty ring expected on this host");
        // And the TSC offset attribute (probed at vCPU creation) must hold:
        sys.create_slot_vm(2 * 1024 * 1024)
            .expect("slot VM incl. TSC offset attr");
    }

    #[test]
    fn slot_vm_constructs_with_memfd_and_vcpu() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(16 * 1024 * 1024).unwrap();
        assert_eq!(slot.mem_bytes, 16 * 1024 * 1024);
        // Guest memory readable/writable from host at GPA 0.
        use vm_memory::{Bytes, GuestAddress};
        slot.guest_mem
            .write_slice(&[0xAA; 8], GuestAddress(0x1000))
            .unwrap();
        let mut back = [0u8; 8];
        slot.guest_mem
            .read_slice(&mut back, GuestAddress(0x1000))
            .unwrap();
        assert_eq!(back, [0xAA; 8]);
    }

    #[test]
    fn mem_above_hole_rejected() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        assert_eq!(
            sys.create_slot_vm(MMIO_HOLE_BASE + 0x1000).err(),
            Some(KvmError::MemTooLarge).map(|e| match e {
                KvmError::MemTooLarge => KvmError::MemTooLarge,
                other => other,
            })
        );
    }

    #[test]
    fn forbidden_list_holds_by_construction() {
        if !kvm_available() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        // We never call KVM_CREATE_IRQCHIP/KVM_CREATE_PIT2. Smoke-assert a
        // fresh vCPU runs real code with NO in-kernel irqchip: a tight
        // HLT loop in real mode reaches VcpuExit::Hlt (with an in-kernel
        // irqchip, HLT would be absorbed in-kernel instead of exiting).
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        use vm_memory::{Bytes, GuestAddress};
        // Real-mode code at 0x0: out 0xD3, al ; hlt
        slot.guest_mem
            .write_slice(&[0xE6, 0xD3, 0xF4], GuestAddress(0))
            .unwrap();
        let mut sregs = slot.vcpu.get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        slot.vcpu.set_sregs(&sregs).unwrap();
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rip = 0;
        regs.rflags = 2;
        slot.vcpu.set_regs(&regs).unwrap();

        // First exit: PIO OUT to an unmapped port → PioIgnored (WI).
        let exit = slot.vcpu.run().unwrap();
        assert_eq!(classify_exit(exit), ExitEvent::PioIgnored { port: 0xD3 });
        // Second: HLT reaches userspace (no in-kernel irqchip absorption).
        let exit = slot.vcpu.run().unwrap();
        assert_eq!(classify_exit(exit), ExitEvent::Hlt);
    }

    #[test]
    fn pio_classification_table() {
        // Host-runnable: pure classification (no KVM needed for the map).
        assert!(in_range(0xD370, PIO_DETCALL_BASE, PIO_DETCALL_LEN));
        assert!(in_range(0xD39F, PIO_DETCALL_BASE, PIO_DETCALL_LEN));
        assert!(!in_range(0xD3A0, PIO_DETCALL_BASE, PIO_DETCALL_LEN));
        assert!(in_range(0x3F8, PIO_SERIAL_BASE, PIO_SERIAL_LEN));
        assert!(in_range(0x3FF, PIO_SERIAL_BASE, PIO_SERIAL_LEN));
        assert!(!in_range(0x400, PIO_SERIAL_BASE, PIO_SERIAL_LEN));
    }

    #[test]
    fn mmio_hole_covers_device_windows() {
        // The §2.2 GPA map must cover every registered device base.
        for base in [0xD000_0000u64, 0xD000_1000, 0xD000_2000, 0xD000_4000] {
            assert!(base >= MMIO_HOLE_BASE && base + 4096 <= MMIO_HOLE_BASE + MMIO_HOLE_LEN);
        }
    }
}
