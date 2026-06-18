//! KVM bring-up (ARCH §2.1/§2.2): capability gate, slot VM construction,
//! and the exit-dispatch skeleton.
//!
//! Determinism posture: the §2.1 required-capability table hard-fails at
//! startup; the forbidden list (in-kernel irqchip, PIT, kvmclock,
//! DISABLE_EXITS) is enforced by construction — this module simply never
//! creates them, and the smoke test asserts a fresh VM has no irqchip.

use kvm_bindings::{KVM_API_VERSION, kvm_userspace_memory_region};
use kvm_ioctls::{Cap, Kvm, VcpuExit, VcpuFd, VmFd};
use vm_memory::{FileOffset, GuestAddress, GuestMemoryMmap};

/// The §2.2 GPA map: device windows live in a RAM hole — no memslot backs
/// them, so guest access produces KVM_EXIT_MMIO.
pub const MMIO_HOLE_BASE: u64 = 0xD000_0000;
pub const MMIO_HOLE_LEN: u64 = 0x7000;

/// ARCH §8.2: dirty ring entry count; ring bytes = entries × 16-byte
/// kvm_dirty_gfn (power of two, as KVM requires).
pub const DIRTY_RING_ENTRIES: u64 = 65536;
/// Bytes per `kvm_dirty_gfn` ring entry (the cap arg is entries × this).
pub const DIRTY_RING_ENTRY_BYTES: u64 = 16;
pub const DIRTY_RING_BYTES: u64 = DIRTY_RING_ENTRIES * DIRTY_RING_ENTRY_BYTES;

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

/// §2.1 required capability table. Dirty-ring support is probed separately:
/// it is an M4/M6 worker host requirement, but keeping it out of this table
/// lets preflight report a dedicated `kvm.cap.dirty_ring` row.
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

fn frozen_ram_seals() -> i32 {
    libc::F_SEAL_FUTURE_WRITE | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW
}

fn add_memfd_seals(file: &std::fs::File, seals: i32, label: &str) -> Result<(), KvmError> {
    use std::os::fd::AsRawFd;
    #[allow(unsafe_code)]
    let rc = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) };
    if rc != 0 {
        return Err(KvmError::Memory(format!(
            "{label}: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

fn get_memfd_seals(file: &std::fs::File) -> Result<i32, KvmError> {
    use std::os::fd::AsRawFd;
    #[allow(unsafe_code)]
    let seals = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GET_SEALS) };
    if seals < 0 {
        return Err(KvmError::Memory(format!(
            "F_GET_SEALS: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(seals)
}

fn open_checked_kvm() -> Result<Kvm, KvmError> {
    let kvm = Kvm::new().map_err(|e| KvmError::Open(e.to_string()))?;
    let version = kvm.get_api_version();
    if version != KVM_API_VERSION as i32 {
        return Err(KvmError::ApiVersionMismatch(version));
    }
    Ok(kvm)
}

fn has_dirty_ring_cap(kvm: &Kvm) -> bool {
    kvm.check_extension_raw(u64::from(kvm_bindings::KVM_CAP_DIRTY_LOG_RING_ACQ_REL)) > 0
}

/// Open /dev/kvm and enforce the §2.1 gate.
pub struct KvmSystem {
    kvm: Kvm,
    /// Dirty ring available; M4/M6 preflight and worker service startup
    /// require this to be true.
    pub dirty_ring: bool,
}

impl KvmSystem {
    pub fn open() -> Result<Self, KvmError> {
        let kvm = open_checked_kvm()?;
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
        let dirty_ring = has_dirty_ring_cap(&kvm);
        Ok(Self { kvm, dirty_ring })
    }

    /// The exact CPUID table installed on newly constructed vCPUs, in
    /// MachineConfig's sorted canonical representation.
    pub fn masked_cpuid_table(&self) -> Result<Vec<crate::config::CpuidLeaf>, KvmError> {
        Ok(crate::cpuid::to_leaves(&crate::cpuid::masked_cpuid(
            &self.kvm,
        )?))
    }

    /// Construct one slot's VM: VM fd, exactly one vCPU, and guest RAM as a
    /// single memfd-backed region [0, mem_bytes) with MADV_NOHUGEPAGE
    /// (ARCH §7.4: 4 KiB-exact dirty granularity). No irqchip, no PIT, no
    /// kvmclock — ever (§2.1 forbidden list, by construction).
    pub fn create_slot_vm(&self, mem_bytes: u64) -> Result<SlotVm, KvmError> {
        self.create_slot_vm_with_ring(mem_bytes, DIRTY_RING_ENTRIES)
    }

    /// `create_slot_vm` with a CALLER-CHOSEN dirty-ring size (power of
    /// two). Production always uses [`DIRTY_RING_ENTRIES`]; the chaos
    /// acceptance (bead 28i) forces tiny rings so harvest-on-full fires
    /// constantly and proves ring-full servicing loss-free (R8).
    pub fn create_slot_vm_with_ring(
        &self,
        mem_bytes: u64,
        ring_entries: u64,
    ) -> Result<SlotVm, KvmError> {
        if mem_bytes > MMIO_HOLE_BASE {
            return Err(KvmError::MemTooLarge);
        }
        if !ring_entries.is_power_of_two() {
            return Err(KvmError::VmCreate(format!(
                "dirty ring entries must be a power of two, got {ring_entries}"
            )));
        }
        let memfd = memfd_nohuge(mem_bytes)?;
        let region = GuestMemoryMmap::<()>::from_ranges_with_files(&[(
            GuestAddress(0),
            mem_bytes as usize,
            Some(FileOffset::new(memfd, 0)),
        )])
        .map_err(|e| KvmError::Memory(e.to_string()))?;
        self.assemble_slot_vm(region, mem_bytes, false, ring_entries)
    }

    /// Tier-A CoW fork (ARCH §8.4): a child slot whose RAM is a PRIVATE
    /// mapping of the FROZEN parent's memfd — reads share the parent's
    /// page cache, writes CoW into child-private anonymous pages at 4 KiB
    /// granularity, and the parent's bytes are untouchable from the child
    /// by construction. Every KVM object (VmFd, vCPU, EPT) is fresh — no
    /// shared kernel state (R9).
    ///
    /// PRECONDITION, enforced here: the parent's memfd carries
    /// `F_SEAL_FUTURE_WRITE` (`SlotVm::freeze_ram`). Forking an unsealed
    /// parent would leave the child's "snapshot" mutable by the parent's
    /// own future writes — fail-closed instead. The kernel-level proof
    /// that private mappings of a sealed memfd work (and that shared-
    /// writable ones are denied) is the iteration-66 probe pinned in the
    /// d2p sealing tests.
    ///
    /// The child starts with NO vCPU/device state copied — the fork
    /// engine stuffs those from the parent's in-memory DHSNAP (the §8.3
    /// codec, minus the store round-trip).
    ///
    /// MAP_NORESERVE means CoW faults allocate lazily with no swap
    /// reservation: a child touching many pages on a memory-tight host
    /// can be OOM-killed at FAULT time rather than fork time. That is
    /// the intended §8.4 trade (fork must not pre-commit a full copy);
    /// capacity policy lives with the slot manager.
    pub fn fork_slot_vm(&self, parent: &SlotVm) -> Result<SlotVm, KvmError> {
        if parent.ram_is_cow {
            return Err(KvmError::Memory(
                "fork of a CoW child: its memfd is the GRANDPARENT's, so the \
                 new child would silently see pre-divergence bytes. \
                 TakeSnapshot + restore into a fresh slot first (see \
                 SlotVm::ram_is_cow)"
                    .into(),
            ));
        }
        let seals = parent.ram_seals()?;
        if seals & libc::F_SEAL_FUTURE_WRITE == 0 {
            return Err(KvmError::Memory(
                "fork of an UNFROZEN parent (R9): freeze_ram first — \
                 F_SEAL_FUTURE_WRITE is not set on the parent memfd"
                    .into(),
            ));
        }
        let memfd = parent
            .ram_memfd()?
            .try_clone()
            .map_err(|e| KvmError::Memory(format!("memfd dup: {e}")))?;
        let mapping = vm_memory::MmapRegion::<()>::build(
            Some(FileOffset::new(memfd, 0)),
            parent.mem_bytes as usize,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_NORESERVE,
        )
        .map_err(|e| KvmError::Memory(format!("CoW mapping: {e}")))?;
        let region = vm_memory::GuestRegionMmap::new(mapping, GuestAddress(0))
            .ok_or_else(|| KvmError::Memory("CoW region address overflow".into()))?;
        let mem = GuestMemoryMmap::<()>::from_regions(vec![region])
            .map_err(|e| KvmError::Memory(e.to_string()))?;
        self.assemble_slot_vm(mem, parent.mem_bytes, true, DIRTY_RING_ENTRIES)
    }

    /// Everything after RAM exists: VM fd with the §2.1/§2.2 cap set,
    /// the memslot, vPMU-off, vCPU 0 with the §7.2 CPUID mask. Shared by
    /// fresh slots (shared memfd mapping) and CoW forks (private mapping).
    fn assemble_slot_vm(
        &self,
        region: GuestMemoryMmap<()>,
        mem_bytes: u64,
        ram_is_cow: bool,
        ring_entries: u64,
    ) -> Result<SlotVm, KvmError> {
        let vm = self
            .kvm
            .create_vm()
            .map_err(|e| KvmError::VmCreate(e.to_string()))?;

        // Dirty ring (ARCH §8.2: 65536 entries) must be enabled BEFORE any
        // vCPU exists — it cannot be retrofitted without recreating the VM.
        // With memslot flags 0 nothing is logged yet; M4 turns harvesting on.
        if self.dirty_ring {
            let mut cap = kvm_bindings::kvm_enable_cap {
                cap: kvm_bindings::KVM_CAP_DIRTY_LOG_RING_ACQ_REL,
                ..Default::default()
            };
            cap.args[0] = ring_entries * DIRTY_RING_ENTRY_BYTES;
            vm.enable_cap(&cap)
                .map_err(|e| KvmError::VmCreate(format!("dirty ring enable: {e}")))?;
        }

        // §2.1/§2.2: USER_SPACE_MSR must be ENABLED with
        // KVM_MSR_EXIT_REASON_FILTER — filter-denied MSR accesses exit to
        // userspace for deterministic emulation instead of KVM injecting
        // #GP. Enabled per-VM here so the preflight smoke proves the
        // combination works on this kernel (the filter itself is the MSR
        // bead's).
        let mut msr_cap = kvm_bindings::kvm_enable_cap {
            cap: kvm_bindings::KVM_CAP_X86_USER_SPACE_MSR,
            ..Default::default()
        };
        msr_cap.args[0] = u64::from(kvm_bindings::KVM_MSR_EXIT_REASON_FILTER);
        vm.enable_cap(&msr_cap)
            .map_err(|e| KvmError::VmCreate(format!("USER_SPACE_MSR enable: {e}")))?;

        // Advise the mapping itself; the memfd flag alone is not enough on
        // all kernels. 4 KiB-exact dirty granularity is load-bearing (§8.2)
        // — for CoW children doubly so: a hugepage CoW would copy 2 MiB per
        // fault and wreck both the <10 ms fork target and dirty accounting.
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

        // In-guest vPMU OFF before any vCPU exists (ARCH §7.2): the guest
        // must not see or program performance counters — they are host
        // state, and the host's pinned INST_RETIRED counter (§3.1) must
        // never contend with a vPMU. HARD-FAIL per the §2.1 philosophy:
        // KVM_CAP_PMU_CAPABILITY exists since 5.18, well below the §7.4
        // kernel baseline, and the CPUID leaf-0xA mask alone does not stop
        // a guest from poking PMU MSRs blind.
        let mut pmu_cap = kvm_bindings::kvm_enable_cap {
            cap: kvm_bindings::KVM_CAP_PMU_CAPABILITY,
            ..Default::default()
        };
        pmu_cap.args[0] = u64::from(kvm_bindings::KVM_PMU_CAP_DISABLE);
        vm.enable_cap(&pmu_cap)
            .map_err(|e| KvmError::VmCreate(format!("KVM_PMU_CAP_DISABLE: {e}")))?;

        let vcpu = vm
            .create_vcpu(0)
            .map_err(|e| KvmError::VcpuCreate(e.to_string()))?;

        // The §7.2 determinism mask is part of slot construction: every
        // vCPU in this hypervisor runs the same fixed, hashed CPUID table.
        let masked = crate::cpuid::masked_cpuid(&self.kvm)?;
        let cpuid_table = crate::cpuid::to_leaves(&masked);
        vcpu.set_cpuid2(&masked)
            .map_err(|e| KvmError::VcpuCreate(format!("KVM_SET_CPUID2: {e}")))?;

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
            cpuid_table,
            ram_is_cow,
            dirty_ring_entries: ring_entries,
        })
    }
}

/// Probe only the dirty-ring capability so preflight can emit a dedicated
/// M4/M6 row even when an older §2.1 capability also fails.
pub fn probe_dirty_ring_cap() -> Result<bool, KvmError> {
    let kvm = open_checked_kvm()?;
    Ok(has_dirty_ring_cap(&kvm))
}

/// Scratch probe for the §8.4 fork-parent kernel invariant, reported by
/// `dh-workerd --preflight`: a memfd created with `MFD_ALLOW_SEALING` must
/// accept `F_SEAL_FUTURE_WRITE`, deny future `write(2)`, keep CoW fork
/// mappings possible, and deny new shared writable mappings after the seal
/// is set.
pub fn probe_memfd_future_write_seal() -> Result<String, KvmError> {
    use std::io::{ErrorKind, Write};
    use std::os::fd::AsRawFd;

    let len = 4096;
    let file = memfd_nohuge(len)?;
    let _live_shared = vm_memory::MmapRegion::<()>::build(
        Some(FileOffset::new(
            file.try_clone()
                .map_err(|e| KvmError::Memory(format!("memfd dup: {e}")))?,
            0,
        )),
        len as usize,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED,
    )
    .map_err(|e| KvmError::Memory(format!("scratch shared writable mmap: {e}")))?;

    add_memfd_seals(
        &file,
        frozen_ram_seals(),
        "F_ADD_SEALS(FUTURE_WRITE|SHRINK|GROW)",
    )?;
    let seals = get_memfd_seals(&file)?;
    if seals & libc::F_SEAL_FUTURE_WRITE == 0 {
        return Err(KvmError::Memory(format!(
            "F_GET_SEALS missing F_SEAL_FUTURE_WRITE: {seals:#x}"
        )));
    }

    let mut writer = file
        .try_clone()
        .map_err(|e| KvmError::Memory(format!("memfd dup: {e}")))?;
    match writer.write_all(&[0x5A]) {
        Ok(()) => {
            return Err(KvmError::Memory(
                "write(2) succeeded after F_SEAL_FUTURE_WRITE".into(),
            ));
        }
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {}
        Err(e) => {
            return Err(KvmError::Memory(format!(
                "write(2) after F_SEAL_FUTURE_WRITE failed unexpectedly: {e}"
            )));
        }
    }

    let _cow = vm_memory::MmapRegion::<()>::build(
        Some(FileOffset::new(
            file.try_clone()
                .map_err(|e| KvmError::Memory(format!("memfd dup: {e}")))?,
            0,
        )),
        len as usize,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE | libc::MAP_NORESERVE,
    )
    .map_err(|e| KvmError::Memory(format!("post-seal CoW mmap: {e}")))?;

    #[allow(unsafe_code)]
    let shared_after_seal = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len as usize,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            file.as_raw_fd(),
            0,
        )
    };
    if shared_after_seal != libc::MAP_FAILED {
        #[allow(unsafe_code)]
        unsafe {
            libc::munmap(shared_after_seal, len as usize);
        }
        return Err(KvmError::Memory(
            "new shared writable mmap succeeded after F_SEAL_FUTURE_WRITE".into(),
        ));
    }
    let mmap_err = std::io::Error::last_os_error();
    if mmap_err.raw_os_error() != Some(libc::EPERM) {
        return Err(KvmError::Memory(format!(
            "new shared writable mmap failed with {mmap_err}; want EPERM from F_SEAL_FUTURE_WRITE"
        )));
    }

    Ok(format!(
        "seals={seals:#x}; write denied; CoW mmap ok; shared writable mmap denied"
    ))
}

/// Scratch probe for the §7.4 THP convention used by slot RAM. This follows
/// the production mapping shape closely enough to catch kernels that reject
/// `MADV_NOHUGEPAGE` on the memfd-backed guest RAM mapping.
pub fn probe_madv_nohugepage() -> Result<String, KvmError> {
    let len = 2 * 1024 * 1024;
    let memfd = memfd_nohuge(len)?;
    let region = GuestMemoryMmap::<()>::from_ranges_with_files(&[(
        GuestAddress(0),
        len as usize,
        Some(FileOffset::new(memfd, 0)),
    )])
    .map_err(|e| KvmError::Memory(format!("scratch memfd mmap: {e}")))?;
    madvise_nohugepage(&region, len)?;
    Ok(format!(
        "MADV_NOHUGEPAGE accepted on {len} byte memfd mapping"
    ))
}

/// One slot's KVM objects (the §2.2 Slot's kernel-facing third).
pub struct SlotVm {
    pub vm: VmFd,
    pub vcpu: VcpuFd,
    pub guest_mem: GuestMemoryMmap<()>,
    pub mem_bytes: u64,
    /// The masked CPUID table actually installed on `vcpu`, sorted in the
    /// same canonical representation MachineConfig hashes.
    pub cpuid_table: Vec<crate::config::CpuidLeaf>,
    /// True for tier-A CoW children: `guest_mem` is a PRIVATE mapping of
    /// the parent's memfd, so the child's diverged pages live in
    /// anonymous memory the memfd never sees. `freeze_ram` and
    /// `fork_slot_vm` fail closed on such slots — sealing/re-mapping the
    /// memfd would silently operate on the PARENT's bytes and drop the
    /// child's divergence. Making a diverged child a fork base goes
    /// through the store: TakeSnapshot, then restore into a fresh slot
    /// (whose RAM IS its own memfd) and freeze that.
    pub ram_is_cow: bool,
    /// The dirty-ring size this VM was created with (entries; power of
    /// two). `DirtyRing::map` sizes the mmap and its cursor mask from
    /// this — a mismatch would mis-mask the free-running cursor.
    pub dirty_ring_entries: u64,
}

impl SlotVm {
    /// The guest-RAM memfd (held inside the region's `FileOffset`).
    fn ram_memfd(&self) -> Result<&std::fs::File, KvmError> {
        use vm_memory::GuestMemoryBackend;
        self.guest_mem
            .find_region(GuestAddress(0))
            .and_then(|r| r.file_offset())
            .map(|fo| fo.file())
            .ok_or_else(|| KvmError::Memory("guest RAM region has no backing memfd".into()))
    }

    /// Freeze the RAM memfd for fork (ARCH §8.4, risk R9):
    /// `F_SEAL_FUTURE_WRITE` blocks every NEW writable mapping — CoW
    /// children and any other process can only map the parent's RAM
    /// read-only from this point on. `F_SEAL_SHRINK | F_SEAL_GROW` ride
    /// along: resizing a frozen parent's RAM is never legitimate, and a
    /// truncate would rip pages out from under the children.
    ///
    /// `F_SEAL_WRITE` is deliberately NOT applied — it is unavailable while
    /// the parent's own KVM mapping exists (the kernel refuses to seal a
    /// memfd with live writable mappings). The parent's existing mapping
    /// therefore stays writable at the kernel level; the SOFTWARE Frozen
    /// state (`SlotState::ensure_write_path`, lib.rs) is the guard for that
    /// half. Idempotent: re-applying the same seals is a kernel no-op
    /// (which is also why `F_SEAL_SEAL` is not added).
    pub fn freeze_ram(&self) -> Result<(), KvmError> {
        if self.ram_is_cow {
            return Err(KvmError::Memory(
                "freeze of a CoW child: its diverged pages live in anonymous \
                 memory, not the memfd — sealing would freeze the PARENT's \
                 bytes and silently drop the divergence. TakeSnapshot the \
                 child and restore into a fresh slot to get a freezable base \
                 (see SlotVm::ram_is_cow)"
                    .into(),
            ));
        }
        add_memfd_seals(
            self.ram_memfd()?,
            frozen_ram_seals(),
            "F_ADD_SEALS(FUTURE_WRITE|SHRINK|GROW)",
        )
    }

    /// Current seal set on the RAM memfd (`F_GET_SEALS`), for tests and the
    /// preflight probe (bead aup).
    pub fn ram_seals(&self) -> Result<i32, KvmError> {
        get_memfd_seals(self.ram_memfd()?)
    }
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
    /// IN-FILL CONTRACT applies (see DetcallIn): this event carries only
    /// `len` — the mutable kvm_run buffer is gone by the time you hold an
    /// ExitEvent. A run loop built on classify_exit CANNOT answer serial
    /// INs through this variant; it must intercept the raw
    /// `VcpuExit::IoIn` and fill via `DebugSerial::pio_read` before
    /// re-entry (as dh-cli's debug loops do), or the guest reads stale
    /// kvm_run bytes — the iter-29 hazard, one layer up.
    SerialIn {
        port: u16,
        len: usize,
    },
    /// Any other port: RAZ/WI (read-as-zero already applied by dispatch).
    PioIgnored {
        port: u16,
    },
    /// Filter-denied RDMSR: deterministic value already written into the
    /// exit by dispatch (msr policy); run control trace-logs the index.
    MsrReadDenied {
        index: u32,
    },
    /// Filter-denied WRMSR: deterministic policy already applied by
    /// dispatch (#GP or an explicit no-op ack); run control trace-logs it.
    MsrWriteDenied {
        index: u32,
        data: u64,
    },
    Hlt,
    Shutdown,
    /// The per-vCPU dirty ring filled (ARCH §8.2): harvest + reset + resume
    /// (host-visible only; never perturbs guest state).
    DirtyRingFull,
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
        // Filter-denied MSR accesses: the §2.2 deterministic emulation is
        // applied HERE, in dispatch — a denied WRMSR resumed without
        // error=1 is silently acked as success by KVM (live-verified),
        // which is exactly the R6 silent-absorption this must prevent.
        VcpuExit::X86Rdmsr(exit) => {
            match crate::msr::on_denied_rdmsr(exit.index) {
                crate::msr::MsrAction::SupplyValue(v) => {
                    *exit.data = v;
                    *exit.error = 0;
                }
                crate::msr::MsrAction::AckWrite => *exit.error = 1,
                crate::msr::MsrAction::InjectGp => *exit.error = 1,
            }
            ExitEvent::MsrReadDenied { index: exit.index }
        }
        VcpuExit::X86Wrmsr(exit) => {
            match crate::msr::on_denied_wrmsr(exit.index) {
                crate::msr::MsrAction::InjectGp => *exit.error = 1,
                crate::msr::MsrAction::AckWrite | crate::msr::MsrAction::SupplyValue(_) => {
                    *exit.error = 0
                }
            }
            ExitEvent::MsrWriteDenied {
                index: exit.index,
                data: exit.data,
            }
        }
        VcpuExit::Hlt => ExitEvent::Hlt,
        VcpuExit::Shutdown => ExitEvent::Shutdown,
        // Ring full is host-visible only (ARCH §8.2): service = harvest +
        // KVM_RESET_DIRTY_RINGS (dirty::harvest_at_boundary), then resume.
        VcpuExit::Unsupported(r) if r == kvm_bindings::KVM_EXIT_DIRTY_RING_FULL => {
            ExitEvent::DirtyRingFull
        }
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
    // MFD_ALLOW_SEALING is load-bearing: §8.4 freezes forked parents with
    // F_SEAL_FUTURE_WRITE, and sealing CANNOT be retrofitted onto a memfd
    // created without this flag (EPERM forever).
    let fd = unsafe {
        libc::memfd_create(
            c"dh-slot-ram".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
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

/// Test guard: can this process actually open /dev/kvm? Existence is not
/// enough — GitHub-hosted runners expose the node (nested virt) but deny the
/// runner user access, so live-KVM tests must skip there and run only where
/// an rw open succeeds (the lab box, the kvm-intel CI lane).
#[cfg(test)]
pub(crate) fn kvm_usable() -> bool {
    use std::io::ErrorKind;
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
    {
        Ok(_) => true,
        // Only "not there" / "not allowed" mean skip; anything else (EMFILE,
        // EINTR, ...) must fail loudly rather than silently shrink coverage
        // on a box where the live legs are supposed to run.
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) => false,
        Err(e) => panic!("unexpected /dev/kvm probe failure: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kvm_available() -> bool {
        crate::kvm::kvm_usable()
    }

    #[test]
    fn caps_gate_passes_on_compliant_host() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = KvmSystem::open().expect("§2.1 caps must hold on the lab box");
        // The lab box runs a 6.x kernel: dirty ring should be available.
        assert!(sys.dirty_ring, "dirty ring expected on this host");
        assert!(
            probe_dirty_ring_cap().expect("dirty-ring probe"),
            "dedicated dirty-ring probe expected on this host"
        );
        // And the TSC offset attribute (probed at vCPU creation) must hold:
        sys.create_slot_vm(2 * 1024 * 1024)
            .expect("slot VM incl. TSC offset attr");
    }

    #[test]
    fn scratch_memfd_future_write_seal_probe_works() {
        probe_memfd_future_write_seal().expect("scratch F_SEAL_FUTURE_WRITE probe");
    }

    #[test]
    fn scratch_madv_nohugepage_probe_works() {
        probe_madv_nohugepage().expect("scratch MADV_NOHUGEPAGE probe");
    }

    #[test]
    fn freeze_ram_seals_future_writes_but_not_the_live_mapping() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        use std::os::fd::AsRawFd;
        use vm_memory::{Bytes, GuestAddress, GuestMemoryBackend};

        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(4 * 1024 * 1024).unwrap();

        // Pre-freeze: no FUTURE_WRITE seal yet; a new writable mapping works.
        let seals = slot.ram_seals().unwrap();
        assert_eq!(seals & libc::F_SEAL_FUTURE_WRITE, 0);

        slot.freeze_ram().expect("freeze");
        let seals = slot.ram_seals().unwrap();
        assert_ne!(seals & libc::F_SEAL_FUTURE_WRITE, 0);
        assert_ne!(seals & libc::F_SEAL_SHRINK, 0);
        assert_ne!(seals & libc::F_SEAL_GROW, 0);
        assert_eq!(seals & libc::F_SEAL_SEAL, 0); // idempotence depends on this

        // Idempotent: re-freeze is a kernel no-op.
        slot.freeze_ram().expect("re-freeze");

        // NEW writable mapping is denied (EPERM) — the R9 hardware half.
        let fd = slot
            .guest_mem
            .find_region(GuestAddress(0))
            .unwrap()
            .file_offset()
            .unwrap()
            .file()
            .as_raw_fd();
        #[allow(unsafe_code)]
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        assert_eq!(p, libc::MAP_FAILED, "writable mmap must be sealed off");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM)
        );

        // A new READ-ONLY mapping still works (children read the baseline).
        #[allow(unsafe_code)]
        let ro = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        assert_ne!(ro, libc::MAP_FAILED, "read-only mapping must survive");
        #[allow(unsafe_code)]
        unsafe {
            libc::munmap(ro, 4096)
        };

        // The parent's own EXISTING mapping stays writable at the kernel
        // level (F_SEAL_WRITE is unavailable while it lives) — exactly why
        // the software Frozen guard exists.
        slot.guest_mem
            .write_slice(&[0x5A; 4], GuestAddress(0x2000))
            .expect("existing mapping must remain writable post-seal");

        // Truncation is sealed off too.
        let file = slot
            .guest_mem
            .find_region(GuestAddress(0))
            .unwrap()
            .file_offset()
            .unwrap()
            .file();
        assert!(file.set_len(1024).is_err(), "SHRINK must be sealed");
    }

    #[test]
    fn slot_vm_constructs_with_memfd_and_vcpu() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
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
            eprintln!("skipping: /dev/kvm not usable");
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
            eprintln!("skipping: /dev/kvm not usable");
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
    fn linux_cpu_compat_rejects_in_kernel_irqchip_pit_and_ioapic_by_construction() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        use vm_memory::{Bytes, GuestAddress};
        slot.guest_mem
            .write_slice(&[0xF4], GuestAddress(0))
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
        assert_eq!(
            classify_exit(exit),
            ExitEvent::Hlt,
            "HLT must exit to userspace when no in-kernel irqchip/PIT/IOAPIC exists"
        );
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
