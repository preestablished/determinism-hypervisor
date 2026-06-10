//! ELF boot path (ARCH §2.3 type 1, bead s0p): PT_LOAD loader, identity
//! 4-level page tables, direct 64-bit entry, BootInfo. Boot is
//! deterministic: same image + same MachineConfig ⇒ same state at any
//! icount — nothing here reads host state.
//!
//! Obligations beyond the M0 dh-cli loader it replaces (iter-29 notes):
//! the MMIO hole (0xD000_0000, §2.2) is mapped in the guest page tables —
//! a device access must reach KVM as an MMIO exit (no memslot), not
//! page-fault into a triple fault — and the §2.2 MSR default-deny filter
//! is applied at boot, so denied MSR accesses exit for deterministic
//! emulation from the first instruction.
//!
//! Layout in low RAM (all below the 1 MiB guest image):
//!   0x1000 PML4   0x2000 PDPT   0x3000..0x7000 four PDs (one per GiB)
//!   0x7000 BootInfo (canonical layout owned by tests/nanokernel lib.rs)

use vm_memory::{Bytes, GuestAddress};

use crate::kvm::{SlotVm, MMIO_HOLE_BASE};

const PML4_GPA: u64 = 0x1000;
const PDPT_GPA: u64 = 0x2000;
const PD_BASE_GPA: u64 = 0x3000; // four consecutive PD pages
/// BootInfo page GPA (RSI at entry). Versioned struct; layout canonical in
/// tests/nanokernel/src/lib.rs.
pub const BOOTINFO_GPA: u64 = 0x7000;
/// The guest image must sit above the loader's low-RAM structures.
const LOW_RAM_RESERVED: u64 = 0x8000;

const PAGE_2M: u64 = 2 << 20;
const GIB: u64 = 1 << 30;
/// Cmdline cap: the BootInfo page is one 4 KiB page.
pub const MAX_CMDLINE: usize = 4096 - 0x20;

#[derive(Debug)]
pub enum BootError {
    Elf(String),
    Mem(String),
    Kvm(String),
    CmdlineTooLong,
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::Elf(e) => write!(f, "elf: {e}"),
            BootError::Mem(e) => write!(f, "guest memory: {e}"),
            BootError::Kvm(e) => write!(f, "kvm: {e}"),
            BootError::CmdlineTooLong => write!(f, "cmdline exceeds {MAX_CMDLINE} bytes"),
        }
    }
}

/// What the loader placed where (diagnostics + harness assertions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootLayout {
    pub entry: u64,
    pub bootinfo_gpa: u64,
}

/// Load `elf`, build page tables + BootInfo, set the MSR filter, and
/// program the vCPU for direct 64-bit entry. The caller owns the run loop.
pub fn load_and_enter(slot: &SlotVm, elf: &[u8], cmdline: &[u8]) -> Result<BootLayout, BootError> {
    let entry = load_elf(&slot.guest_mem, elf)?;
    write_page_tables(&slot.guest_mem, slot.mem_bytes)?;
    write_bootinfo(&slot.guest_mem, slot.mem_bytes, cmdline)?;
    crate::msr::apply_default_deny_filter(&slot.vm).map_err(|e| BootError::Kvm(e.0))?;
    enter_long_mode(&slot.vcpu, entry)?;
    Ok(BootLayout {
        entry,
        bootinfo_gpa: BOOTINFO_GPA,
    })
}

/// Copy PT_LOAD segments into guest RAM with explicit [filesz, memsz)
/// zero-fill (the nanokernel loader contract — guests keep stacks in bss).
pub fn load_elf<M>(mem: &M, elf: &[u8]) -> Result<u64, BootError>
where
    M: Bytes<GuestAddress, E = vm_memory::GuestMemoryError>,
{
    let bad = |m: &str| BootError::Elf(m.into());
    let u16le = |at: usize| -> Option<u16> {
        Some(u16::from_le_bytes(elf.get(at..at + 2)?.try_into().ok()?))
    };
    let u64le = |at: usize| -> Option<u64> {
        Some(u64::from_le_bytes(elf.get(at..at + 8)?.try_into().ok()?))
    };
    if elf.get(0..4) != Some(b"\x7fELF") || elf.get(4) != Some(&2) || elf.get(5) != Some(&1) {
        return Err(bad("not a little-endian ELF64"));
    }
    if u16le(16) != Some(2) || u16le(18) != Some(62) {
        return Err(bad("not an x86_64 ET_EXEC image"));
    }
    let entry = u64le(24).ok_or_else(|| bad("truncated header"))?;
    let phoff = u64le(32).ok_or_else(|| bad("truncated header"))? as usize;
    let phentsize = u16le(54).ok_or_else(|| bad("truncated header"))? as usize;
    let phnum = u16le(56).ok_or_else(|| bad("truncated header"))? as usize;
    let mut any_load = false;
    for i in 0..phnum {
        let at = phoff + i * phentsize;
        let p_type = u32::from_le_bytes(
            elf.get(at..at + 4)
                .ok_or_else(|| bad("truncated phdr"))?
                .try_into()
                .unwrap(),
        );
        if p_type != 1 {
            continue;
        }
        let p_offset = u64le(at + 8).ok_or_else(|| bad("truncated phdr"))? as usize;
        let p_vaddr = u64le(at + 16).ok_or_else(|| bad("truncated phdr"))?;
        let p_filesz = u64le(at + 32).ok_or_else(|| bad("truncated phdr"))? as usize;
        let p_memsz = u64le(at + 40).ok_or_else(|| bad("truncated phdr"))?;
        if p_vaddr < LOW_RAM_RESERVED {
            return Err(bad("PT_LOAD overlaps the loader's low-RAM structures"));
        }
        let file_bytes = elf
            .get(
                p_offset
                    ..p_offset
                        .checked_add(p_filesz)
                        .ok_or_else(|| bad("phdr overflow"))?,
            )
            .ok_or_else(|| bad("PT_LOAD beyond file end"))?;
        mem.write_slice(file_bytes, GuestAddress(p_vaddr))
            .map_err(|e| BootError::Mem(format!("PT_LOAD copy: {e}")))?;
        let tail = p_memsz.saturating_sub(p_filesz as u64) as usize;
        if tail > 0 {
            mem.write_slice(&vec![0u8; tail], GuestAddress(p_vaddr + p_filesz as u64))
                .map_err(|e| BootError::Mem(format!("bss zero-fill: {e}")))?;
        }
        any_load = true;
    }
    if !any_load {
        return Err(bad("no PT_LOAD segments"));
    }
    Ok(entry)
}

/// Identity-map [0, mem_bytes) plus one 2 MiB page covering the MMIO hole.
/// Four PDs cover GPA 0..4 GiB; RAM is capped at MMIO_HOLE_BASE by
/// create_slot_vm, so RAM and the hole both fit.
pub fn write_page_tables<M>(mem: &M, mem_bytes: u64) -> Result<(), BootError>
where
    M: Bytes<GuestAddress, E = vm_memory::GuestMemoryError>,
{
    let w = |gpa: u64, v: u64| {
        mem.write_slice(&v.to_le_bytes(), GuestAddress(gpa))
            .map_err(|e| BootError::Mem(format!("page table write: {e}")))
    };
    w(PML4_GPA, PDPT_GPA | 0b11)?; // present | writable
    for gib in 0..4u64 {
        w(PDPT_GPA + gib * 8, (PD_BASE_GPA + gib * 0x1000) | 0b11)?;
    }
    // RAM: 2 MiB pages, present | writable | PS.
    for page in 0..mem_bytes.div_ceil(PAGE_2M) {
        let gpa = page * PAGE_2M;
        let pd = PD_BASE_GPA + (gpa / GIB) * 0x1000;
        w(pd + ((gpa % GIB) / PAGE_2M) * 8, gpa | 0b1000_0011)?;
    }
    // The MMIO hole: PTE present so device accesses reach KVM as MMIO
    // exits (no memslot there), never as guest page faults.
    let hole_pd = PD_BASE_GPA + (MMIO_HOLE_BASE / GIB) * 0x1000;
    w(
        hole_pd + ((MMIO_HOLE_BASE % GIB) / PAGE_2M) * 8,
        MMIO_HOLE_BASE | 0b1000_0011,
    )?;
    Ok(())
}

/// The canonical BootInfo page (layout owned by tests/nanokernel lib.rs).
pub fn write_bootinfo<M>(mem: &M, mem_bytes: u64, cmdline: &[u8]) -> Result<(), BootError>
where
    M: Bytes<GuestAddress, E = vm_memory::GuestMemoryError>,
{
    if cmdline.len() > MAX_CMDLINE {
        return Err(BootError::CmdlineTooLong);
    }
    let mut page = Vec::with_capacity(0x20 + cmdline.len());
    page.extend_from_slice(b"DHBI");
    page.extend_from_slice(&1u32.to_le_bytes());
    page.extend_from_slice(&mem_bytes.to_le_bytes());
    page.extend_from_slice(&MMIO_HOLE_BASE.to_le_bytes());
    page.extend_from_slice(&(cmdline.len() as u32).to_le_bytes());
    page.extend_from_slice(&0u32.to_le_bytes());
    page.extend_from_slice(cmdline);
    mem.write_slice(&page, GuestAddress(BOOTINFO_GPA))
        .map_err(|e| BootError::Mem(format!("bootinfo write: {e}")))
}

/// Direct 64-bit entry (§2.3): CR0/CR3/CR4/EFER + segment caches via
/// KVM_SET_SREGS (no real-mode phase, no in-memory GDT — KVM honors the
/// cached descriptors), RIP = e_entry, RSI = &BootInfo, RFLAGS = 2.
pub fn enter_long_mode(vcpu: &kvm_ioctls::VcpuFd, entry: u64) -> Result<(), BootError> {
    let kvm_err = |e: kvm_ioctls::Error| BootError::Kvm(e.to_string());
    let mut sregs = vcpu.get_sregs().map_err(kvm_err)?;

    let code = kvm_bindings::kvm_segment {
        base: 0,
        limit: 0xf_ffff,
        selector: 0x08,
        type_: 0xb, // execute/read, accessed
        present: 1,
        dpl: 0,
        db: 0,
        s: 1,
        l: 1, // 64-bit
        g: 1,
        avl: 0,
        unusable: 0,
        padding: 0,
    };
    let data = kvm_bindings::kvm_segment {
        selector: 0x10,
        type_: 0x3, // read/write, accessed
        l: 0,
        db: 1,
        ..code
    };
    sregs.cs = code;
    sregs.ds = data;
    sregs.es = data;
    sregs.fs = data;
    sregs.gs = data;
    sregs.ss = data;
    sregs.cr3 = PML4_GPA;
    sregs.cr4 = 1 << 5; // PAE
    sregs.cr0 = 0x8000_0021; // PG | NE | PE
    sregs.efer = (1 << 8) | (1 << 10); // LME | LMA
    vcpu.set_sregs(&sregs).map_err(kvm_err)?;

    let mut regs = vcpu.get_regs().map_err(kvm_err)?;
    regs.rip = entry;
    regs.rsi = BOOTINFO_GPA;
    regs.rflags = 2;
    vcpu.set_regs(&regs).map_err(kvm_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_memory::GuestMemoryMmap;

    /// Host-side memory (no /dev/kvm needed): plain anonymous mmap.
    fn ram(bytes: u64) -> GuestMemoryMmap<()> {
        GuestMemoryMmap::from_ranges(&[(GuestAddress(0), bytes as usize)]).unwrap()
    }

    fn read_u64(mem: &GuestMemoryMmap<()>, gpa: u64) -> u64 {
        let mut b = [0u8; 8];
        mem.read_slice(&mut b, GuestAddress(gpa)).unwrap();
        u64::from_le_bytes(b)
    }

    #[test]
    fn page_tables_map_ram_and_the_mmio_hole() {
        let mem = ram(64 << 20); // 64 MiB
        write_page_tables(&mem, 64 << 20).unwrap();
        // PML4[0] -> PDPT; PDPT[0..4] -> PDs.
        assert_eq!(read_u64(&mem, PML4_GPA), PDPT_GPA | 0b11);
        assert_eq!(read_u64(&mem, PDPT_GPA), PD_BASE_GPA | 0b11);
        assert_eq!(
            read_u64(&mem, PDPT_GPA + 3 * 8),
            (PD_BASE_GPA + 0x3000) | 0b11
        );
        // First and last RAM 2 MiB pages present|rw|ps.
        assert_eq!(read_u64(&mem, PD_BASE_GPA), 0b1000_0011);
        let last = (64 << 20) / PAGE_2M - 1;
        assert_eq!(
            read_u64(&mem, PD_BASE_GPA + last * 8),
            (last * PAGE_2M) | 0b1000_0011
        );
        // The MMIO hole PTE lives in PD #3 (GPA 0xD000_0000).
        let hole_pd = PD_BASE_GPA + (MMIO_HOLE_BASE / GIB) * 0x1000;
        let hole_slot = hole_pd + ((MMIO_HOLE_BASE % GIB) / PAGE_2M) * 8;
        assert_eq!(read_u64(&mem, hole_slot), MMIO_HOLE_BASE | 0b1000_0011);
    }

    #[test]
    fn bootinfo_layout_matches_the_canonical_abi() {
        let mem = ram(16 << 20);
        write_bootinfo(&mem, 16 << 20, b"42").unwrap();
        let mut page = [0u8; 0x24];
        mem.read_slice(&mut page, GuestAddress(BOOTINFO_GPA))
            .unwrap();
        assert_eq!(&page[0..4], b"DHBI");
        assert_eq!(u32::from_le_bytes(page[4..8].try_into().unwrap()), 1);
        assert_eq!(
            u64::from_le_bytes(page[8..16].try_into().unwrap()),
            16 << 20
        );
        assert_eq!(
            u64::from_le_bytes(page[16..24].try_into().unwrap()),
            MMIO_HOLE_BASE
        );
        assert_eq!(u32::from_le_bytes(page[24..28].try_into().unwrap()), 2);
        assert_eq!(&page[0x20..0x22], b"42");

        assert!(matches!(
            write_bootinfo(&mem, 16 << 20, &vec![b'x'; MAX_CMDLINE + 1]),
            Err(BootError::CmdlineTooLong)
        ));
    }

    #[test]
    fn elf_loader_copies_and_zero_fills() {
        // Hand-built minimal ELF64: one PT_LOAD at 0x100000, filesz 4,
        // memsz 16, entry 0x100000.
        let mut elf = vec![0u8; 0x78 + 4];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        elf[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
        elf[24..32].copy_from_slice(&0x10_0000u64.to_le_bytes()); // entry
        elf[32..40].copy_from_slice(&0x40u64.to_le_bytes()); // phoff
        elf[54..56].copy_from_slice(&56u16.to_le_bytes()); // phentsize
        elf[56..58].copy_from_slice(&1u16.to_le_bytes()); // phnum
                                                          // phdr at 0x40: PT_LOAD, offset 0x78, vaddr 0x100000, filesz 4, memsz 16
        elf[0x40..0x44].copy_from_slice(&1u32.to_le_bytes());
        elf[0x48..0x50].copy_from_slice(&0x78u64.to_le_bytes());
        elf[0x50..0x58].copy_from_slice(&0x10_0000u64.to_le_bytes());
        elf[0x60..0x68].copy_from_slice(&4u64.to_le_bytes());
        elf[0x68..0x70].copy_from_slice(&16u64.to_le_bytes());
        elf[0x78..0x7C].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let mem = ram(4 << 20);
        // Pre-dirty the bss range to prove explicit zero-fill.
        mem.write_slice(&[0xFFu8; 16], GuestAddress(0x10_0000))
            .unwrap();
        let entry = load_elf(&mem, &elf).unwrap();
        assert_eq!(entry, 0x10_0000);
        let mut got = [0u8; 16];
        mem.read_slice(&mut got, GuestAddress(0x10_0000)).unwrap();
        assert_eq!(&got[..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(&got[4..], &[0u8; 12], "bss must be zero-filled");

        // Rejections: PT_LOAD into loader-reserved low RAM; truncated file.
        let mut low = elf.clone();
        low[0x50..0x58].copy_from_slice(&0x2000u64.to_le_bytes());
        assert!(load_elf(&mem, &low).is_err());
        assert!(load_elf(&mem, &elf[..0x50]).is_err());
    }
}
