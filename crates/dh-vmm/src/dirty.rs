//! Dirty-ring harvest (ARCH §8.2, risk R8) — the per-vCPU
//! `KVM_CAP_DIRTY_LOG_RING_ACQ_REL` ring, drained at every pause boundary.
//!
//! Protocol (the ACQ_REL flavor, which the workspace requires at VM
//! creation): KVM publishes `kvm_dirty_gfn` entries with a store-release of
//! `flags = DIRTY`; userspace harvests with a load-acquire, records the
//! GFN, and store-releases `flags = RESET`; `KVM_RESET_DIRTY_RINGS` (a VM
//! ioctl) then collects RESET entries and re-arms their ring slots. The
//! free-running cursor (`next_harvest`) never rewinds: ring slots are
//! consumed strictly in order, so a harvest at a pause boundary plus the
//! same call on `KVM_EXIT_DIRTY_RING_FULL` is loss-free by construction —
//! KVM never overwrites an un-RESET entry: it exits ring-full (at the
//! kernel's soft-full watermark, which leaves headroom for in-flight
//! dirtying) and the vCPU cannot re-enter until a reset frees slots.
//!
//! The dirty set itself is a dense per-slot bitmap ([`DirtyPageSet`]).
//! ARCHITECTURE §8.2 sketches `RoaringBitmap<page_idx>`; v1 guests are
//! ≤ 3 GiB (≤ 786k pages ⇒ ≤ 96 KiB of bitmap), where a dense bitmap is
//! smaller and simpler than a roaring dep. The API is shaped so swapping
//! the representation later is invisible to the snapshot engine.

use crate::kvm::{KvmError, SlotVm, DIRTY_RING_ENTRIES};
use kvm_bindings::{kvm_dirty_gfn, kvm_userspace_memory_region, KVM_DIRTY_LOG_PAGE_OFFSET};
use kvm_ioctls::{VcpuFd, VmFd};
use std::sync::atomic::{AtomicU32, Ordering};

/// 4 KiB pages — the §7.4 MADV_NOHUGEPAGE invariant makes this exact.
/// (hash.rs carries a usize twin for its slice math; both are pinned to
/// the architectural page size, not tunable.)
pub const PAGE_SIZE: u64 = 4096;

// kvm_dirty_gfn.flags bits (kernel ABI; kvm-bindings 0.14 exports only
// KVM_DIRTY_GFN_F_MASK = 3, which pins these two).
const DIRTY_GFN_F_DIRTY: u32 = 1 << 0;
const DIRTY_GFN_F_RESET: u32 = 1 << 1;

/// `KVM_RESET_DIRTY_RINGS` = `_IO(KVMIO, 0xc7)`; kvm-ioctls 0.24 has no
/// wrapper. `_IO` encodes dir=NONE size=0 type=0xAE nr=0xc7.
const KVM_RESET_DIRTY_RINGS: libc::c_ulong = 0xAEC7;

/// The mmap'd per-vCPU dirty ring plus its free-running harvest cursor.
pub struct DirtyRing {
    ring: *mut kvm_dirty_gfn,
    map_len: usize,
    next_harvest: u64,
}

// SAFETY: the mapping is private to this struct (single owner); entries are
// only touched through atomic acquire/release accesses per the KVM ACQ_REL
// contract, so moving the owner across threads is sound.
#[allow(unsafe_code)]
unsafe impl Send for DirtyRing {}

impl DirtyRing {
    /// mmap the vCPU's dirty ring (`KVM_DIRTY_LOG_PAGE_OFFSET` pages into
    /// the vCPU fd). The VM must have been created with the dirty ring
    /// enabled (kvm.rs does this before any vCPU exists).
    pub fn map(vcpu: &VcpuFd) -> Result<Self, KvmError> {
        use std::os::fd::AsRawFd;
        let map_len = (DIRTY_RING_ENTRIES as usize) * std::mem::size_of::<kvm_dirty_gfn>();
        #[allow(unsafe_code)]
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                vcpu.as_raw_fd(),
                (u64::from(KVM_DIRTY_LOG_PAGE_OFFSET) * PAGE_SIZE) as libc::off_t,
            )
        };
        if p == libc::MAP_FAILED {
            return Err(KvmError::Memory(format!(
                "dirty ring mmap: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(Self {
            ring: p.cast::<kvm_dirty_gfn>(),
            map_len,
            next_harvest: 0,
        })
    }

    /// Drain every published entry into `set`, marking each RESET. Returns
    /// the number harvested. Caller follows up with [`reset_dirty_rings`]
    /// (per ARCH §8.2: drain → reset) — entries are not reusable by KVM
    /// until then.
    ///
    /// ERROR CONTRACT (for the snapshot engine, bead qmp): a mid-harvest
    /// error (unexpected memslot, out-of-range GFN) is TERMINAL for the
    /// slot — it means KVM and our memory model disagree. Do not build a
    /// retry-the-boundary loop on it; destroy/restore the slot. (Entries
    /// already marked RESET before the error are reaped by the kernel on
    /// the next reset ioctl; nothing is stranded — verified empirically.)
    pub fn harvest_into(&mut self, set: &mut DirtyPageSet) -> Result<u32, KvmError> {
        let mut harvested = 0u32;
        loop {
            let idx = (self.next_harvest % DIRTY_RING_ENTRIES) as usize;
            // SAFETY: idx < DIRTY_RING_ENTRIES bounds the mapping; the
            // flags word is accessed atomically per the ACQ_REL contract
            // (KVM writes it concurrently from the vCPU side).
            #[allow(unsafe_code)]
            let (flags_atomic, slot, offset) = unsafe {
                let entry = self.ring.add(idx);
                let flags = &*std::ptr::addr_of!((*entry).flags).cast::<AtomicU32>();
                // slot/offset are read AFTER the acquire load below; KVM
                // wrote them before its release store of DIRTY.
                (
                    flags,
                    std::ptr::addr_of!((*entry).slot),
                    std::ptr::addr_of!((*entry).offset),
                )
            };
            if flags_atomic.load(Ordering::Acquire) & DIRTY_GFN_F_DIRTY == 0 {
                break; // next unpublished entry — ring drained
            }
            #[allow(unsafe_code)]
            let (slot, gfn) = unsafe { (slot.read(), offset.read()) };
            // One memslot (id 0, address space 0) in v1 — anything else is
            // a contract violation, not a skippable curiosity.
            if slot != 0 {
                return Err(KvmError::Memory(format!(
                    "dirty ring entry for unexpected memslot {slot:#x} (gfn {gfn:#x})"
                )));
            }
            set.insert(gfn)?;
            flags_atomic.store(DIRTY_GFN_F_RESET, Ordering::Release);
            self.next_harvest += 1;
            harvested += 1;
        }
        Ok(harvested)
    }
}

impl Drop for DirtyRing {
    fn drop(&mut self) {
        // SAFETY: unmapping exactly what map() mapped; the pointer is not
        // used after Drop.
        #[allow(unsafe_code)]
        unsafe {
            libc::munmap(self.ring.cast(), self.map_len);
        }
    }
}

/// `KVM_RESET_DIRTY_RINGS`: collect RESET entries VM-wide and re-arm their
/// ring slots. Returns the count the kernel processed.
pub fn reset_dirty_rings(vm: &VmFd) -> Result<u32, KvmError> {
    use std::os::fd::AsRawFd;
    // SAFETY: _IO ioctl with no argument on a VM fd we own.
    #[allow(unsafe_code)]
    let rc = unsafe { libc::ioctl(vm.as_raw_fd(), KVM_RESET_DIRTY_RINGS) };
    if rc < 0 {
        return Err(KvmError::Memory(format!(
            "KVM_RESET_DIRTY_RINGS: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(rc as u32)
}

/// Turn dirty logging ON for the slot's RAM memslot (M4 "turns harvesting
/// on"): re-issues the memslot with `KVM_MEM_LOG_DIRTY_PAGES`. Without the
/// flag the ring stays empty; with it every guest write publishes a ring
/// entry. Idempotent.
pub fn enable_dirty_logging(slot: &SlotVm) -> Result<(), KvmError> {
    set_ram_flags(slot, kvm_bindings::KVM_MEM_LOG_DIRTY_PAGES)
}

fn set_ram_flags(slot: &SlotVm, flags: u32) -> Result<(), KvmError> {
    use vm_memory::{GuestAddress, GuestMemoryBackend};
    let userspace_addr = slot
        .guest_mem
        .get_host_address(GuestAddress(0))
        .map_err(|e| KvmError::Memory(e.to_string()))? as u64;
    let region = kvm_userspace_memory_region {
        slot: 0,
        guest_phys_addr: 0,
        memory_size: slot.mem_bytes,
        userspace_addr,
        flags,
    };
    // SAFETY: same contract as create_slot_vm's registration — the region
    // outlives the VM via SlotVm holding both; only flags change.
    #[allow(unsafe_code)]
    unsafe { slot.vm.set_user_memory_region(region) }
        .map_err(|e| KvmError::Memory(format!("set memslot flags: {e}")))
}

/// One pause-boundary drain (ARCH §8.2): harvest the ring into `set`, then
/// `KVM_RESET_DIRTY_RINGS`. Also the loss-free `KVM_EXIT_DIRTY_RING_FULL`
/// service path — same call, then re-enter the guest.
pub fn harvest_at_boundary(
    ring: &mut DirtyRing,
    vm: &VmFd,
    set: &mut DirtyPageSet,
) -> Result<HarvestStats, KvmError> {
    let harvested = ring.harvest_into(set)?;
    let reset = if harvested > 0 {
        reset_dirty_rings(vm)?
    } else {
        0
    };
    Ok(HarvestStats { harvested, reset })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HarvestStats {
    pub harvested: u32,
    /// Entries the kernel re-armed; equals `harvested` in single-vCPU v1.
    pub reset: u32,
}

/// Dense page-index bitmap accumulated since the last TakeSnapshot.
/// Deterministic ascending iteration (the manifest entry order depends on
/// it). Out-of-range GFNs are loud errors: with one memslot covering
/// [0, mem_bytes), KVM cannot legitimately report a GFN past the end.
#[derive(Clone, Debug)]
pub struct DirtyPageSet {
    bits: Vec<u64>,
    pages: u64,
    set_count: u64,
}

impl DirtyPageSet {
    pub fn new(mem_bytes: u64) -> Self {
        let pages = mem_bytes.div_ceil(PAGE_SIZE);
        Self {
            bits: vec![0u64; pages.div_ceil(64) as usize],
            pages,
            set_count: 0,
        }
    }

    pub fn insert(&mut self, page_idx: u64) -> Result<bool, KvmError> {
        if page_idx >= self.pages {
            return Err(KvmError::Memory(format!(
                "dirty gfn {page_idx:#x} beyond guest RAM ({} pages)",
                self.pages
            )));
        }
        let (word, bit) = ((page_idx / 64) as usize, page_idx % 64);
        let newly = self.bits[word] & (1 << bit) == 0;
        self.bits[word] |= 1 << bit;
        self.set_count += u64::from(newly);
        Ok(newly)
    }

    pub fn contains(&self, page_idx: u64) -> bool {
        page_idx < self.pages && self.bits[(page_idx / 64) as usize] & (1 << (page_idx % 64)) != 0
    }

    pub fn len(&self) -> u64 {
        self.set_count
    }

    pub fn is_empty(&self) -> bool {
        self.set_count == 0
    }

    /// Cleared after a successful TakeSnapshot (ARCH §8.2).
    pub fn clear(&mut self) {
        self.bits.fill(0);
        self.set_count = 0;
    }

    /// Ascending page indices — the deterministic manifest order.
    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        self.bits.iter().enumerate().flat_map(|(w, &word)| {
            (0..64)
                .filter(move |b| word & (1 << b) != 0)
                .map(move |b| (w as u64) * 64 + b)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_set_inserts_iterates_ascending_and_clears() {
        let mut s = DirtyPageSet::new(128 * 1024 * 1024); // 32768 pages
        assert!(s.is_empty());
        for idx in [9u64, 2, 5, 2, 32767] {
            s.insert(idx).unwrap();
        }
        assert_eq!(s.len(), 4); // duplicate 2 counted once
        assert!(s.contains(2) && s.contains(5) && s.contains(9) && s.contains(32767));
        assert!(!s.contains(3));
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![2, 5, 9, 32767]);
        assert!(s.insert(32768).is_err()); // beyond RAM: loud
        s.clear();
        assert!(s.is_empty());
        assert_eq!(s.iter().count(), 0);
    }

    #[test]
    fn page_set_handles_non_page_multiple_ram() {
        let s = DirtyPageSet::new(4096 * 3 + 1); // 4 pages
        assert_eq!(s.pages, 4);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::kvm::{classify_exit, ExitEvent, KvmSystem};
    use vm_memory::{Bytes, GuestAddress};

    fn kvm_available() -> bool {
        crate::kvm::kvm_usable()
    }

    #[test]
    fn fresh_ring_harvests_nothing() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        let mut ring = DirtyRing::map(&slot.vcpu).expect("ring mmap");
        let mut set = DirtyPageSet::new(slot.mem_bytes);
        let stats = harvest_at_boundary(&mut ring, &slot.vm, &mut set).unwrap();
        assert_eq!(stats, HarvestStats::default());
        assert!(set.is_empty());
    }

    /// End-to-end ARCH §8.2: guest writes land in the ring, harvest
    /// collects exactly those GFNs, reset re-arms, and a second run/harvest
    /// cycle still works (the cursor keeps advancing).
    #[test]
    fn guest_writes_are_harvested_and_ring_resets() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        let mut ring = DirtyRing::map(&slot.vcpu).expect("ring mmap");
        let mut set = DirtyPageSet::new(slot.mem_bytes);
        enable_dirty_logging(&slot).expect("memslot dirty logging on");

        // Real-mode code at 0x0:
        //   mov byte [0x2000], 0x42 ; C6 06 00 20 42
        //   mov byte [0x5000], 0x43 ; C6 06 00 50 43
        //   mov byte [0x9000], 0x44 ; C6 06 00 90 44
        //   hlt                     ; F4
        slot.guest_mem
            .write_slice(
                &[
                    0xC6, 0x06, 0x00, 0x20, 0x42, //
                    0xC6, 0x06, 0x00, 0x50, 0x43, //
                    0xC6, 0x06, 0x00, 0x90, 0x44, //
                    0xF4,
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

        // Run to HLT, servicing any ring-full on the way (2 MiB guest,
        // 65536-entry ring: full is impossible here, but the loop IS the
        // documented service shape).
        loop {
            let exit = slot.vcpu.run().unwrap();
            match classify_exit(exit) {
                ExitEvent::Hlt => break,
                ExitEvent::DirtyRingFull => {
                    harvest_at_boundary(&mut ring, &slot.vm, &mut set).unwrap();
                }
                other => panic!("unexpected exit: {other:?}"),
            }
        }

        let stats = harvest_at_boundary(&mut ring, &slot.vm, &mut set).unwrap();
        assert!(stats.harvested >= 3, "stats: {stats:?}");
        assert_eq!(stats.harvested, stats.reset);
        // The three written pages MUST be present (other pages may also be
        // dirty — e.g. the page KVM used for instruction emulation state).
        for page in [0x2u64, 0x5, 0x9] {
            assert!(set.contains(page), "page {page:#x} missing: {set:?}");
        }

        // Cycle 2: the cursor advances past reset entries — write one more
        // page and harvest again on the re-armed ring.
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rip = 0x100;
        slot.vcpu.set_regs(&regs).unwrap();
        slot.guest_mem
            .write_slice(
                &[0xC6, 0x06, 0x00, 0x70, 0x45, 0xF4], // mov byte [0x7000], 0x45 ; hlt
                GuestAddress(0x100),
            )
            .unwrap();
        loop {
            let exit = slot.vcpu.run().unwrap();
            match classify_exit(exit) {
                ExitEvent::Hlt => break,
                ExitEvent::DirtyRingFull => {
                    harvest_at_boundary(&mut ring, &slot.vm, &mut set).unwrap();
                }
                other => panic!("unexpected exit: {other:?}"),
            }
        }
        let before = set.len();
        let stats2 = harvest_at_boundary(&mut ring, &slot.vm, &mut set).unwrap();
        assert!(stats2.harvested >= 1, "stats2: {stats2:?}");
        assert!(set.contains(0x7), "page 0x7 missing after cycle 2");
        assert!(set.len() > before);

        // Snapshot-boundary semantics: clear starts the next segment's set.
        set.clear();
        assert!(set.is_empty());
    }
}
