//! M0 boot path (bead 1mz): a MINIMAL loader + run-until-HLT loop able to
//! boot the nanokernel stubs and capture their debug-serial bytes.
//!
//! This is deliberately the small M0 cousin of the real ELF boot path
//! (bead s0p, dh-vmm): identity 2 MiB page tables covering guest RAM only
//! (≤ 1 GiB, so the MMIO hole is NOT mapped — fine for hello, not for the
//! device-exercise guest), direct long-mode entry via KVM_SET_SREGS, the
//! canonical BootInfo page, and a dumb exit loop: serial OUT bytes are
//! collected, every IN in the serial/detcall windows reads as zeros (the
//! classify_exit IN-FILL contract — the buffer must be written before
//! re-entry, so INs are answered HERE on the raw exit, before
//! classify_exit ever sees them), HLT ends the run.

use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem};
use kvm_bindings::kvm_segment;
use kvm_ioctls::VcpuExit;
use vm_memory::{Bytes, GuestAddress};

/// Page-table GPAs (low RAM, below the 1 MiB guest image).
const PML4_GPA: u64 = 0x1000;
const PDPT_GPA: u64 = 0x2000;
const PD_GPA: u64 = 0x3000;
/// BootInfo page (RSI at entry).
const BOOTINFO_GPA: u64 = 0x5000;

const SERIAL_BASE: u16 = 0x3F8;
const SERIAL_END: u16 = 0x400;

pub struct BootOutcome {
    /// Bytes the guest wrote to the debug serial port, in order.
    pub serial: Vec<u8>,
    /// Total VM exits consumed.
    pub exits: u64,
}

#[derive(Debug)]
pub enum BootError {
    Kvm(String),
    Elf(String),
    Mem(String),
    /// The guest did something the M0 loop does not model.
    UnexpectedExit(String),
    /// max_exits consumed without reaching HLT.
    ExitBudget,
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::Kvm(e) => write!(f, "kvm: {e}"),
            BootError::Elf(e) => write!(f, "elf: {e}"),
            BootError::Mem(e) => write!(f, "guest memory: {e}"),
            BootError::UnexpectedExit(e) => write!(f, "unexpected exit: {e}"),
            BootError::ExitBudget => write!(f, "exit budget exhausted before HLT"),
        }
    }
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn u64le(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Boot `elf` with `mem_bytes` of RAM and run until HLT (or the exit
/// budget). Returns the serial bytes the guest produced.
pub fn boot(
    elf: &[u8],
    mem_bytes: u64,
    cmdline: &[u8],
    max_exits: u64,
) -> Result<BootOutcome, BootError> {
    if mem_bytes > 1 << 30 {
        return Err(BootError::Mem(
            "M0 loader maps at most 1 GiB (one page directory)".into(),
        ));
    }
    let sys = KvmSystem::open().map_err(|e| BootError::Kvm(format!("{e:?}")))?;
    let slot = sys
        .create_slot_vm(mem_bytes)
        .map_err(|e| BootError::Kvm(format!("{e:?}")))?;

    let entry = load_elf(&slot.guest_mem, elf)?;
    write_page_tables(&slot.guest_mem, mem_bytes)?;
    write_bootinfo(&slot.guest_mem, mem_bytes, cmdline)?;
    enter_long_mode(&slot.vcpu, entry)?;
    run_until_hlt(slot, max_exits)
}

/// Copy PT_LOAD segments into guest RAM. Fresh guest RAM is zeroed, which
/// covers the [filesz, memsz) zero-fill obligation (nanokernel lib.rs
/// loader contract) — asserted here anyway by zeroing explicitly so a
/// future memslot reuse cannot regress it.
fn load_elf(
    mem: &impl Bytes<GuestAddress, E = vm_memory::GuestMemoryError>,
    elf: &[u8],
) -> Result<u64, BootError> {
    let bad = |m: &str| BootError::Elf(m.into());
    if elf.get(0..4) != Some(b"\x7fELF") || elf.get(4) != Some(&2) || elf.get(5) != Some(&1) {
        return Err(bad("not a little-endian ELF64"));
    }
    if u16le(elf, 16) != Some(2) || u16le(elf, 18) != Some(62) {
        return Err(bad("not an x86_64 ET_EXEC image"));
    }
    let entry = u64le(elf, 24).ok_or_else(|| bad("truncated header"))?;
    let phoff = u64le(elf, 32).ok_or_else(|| bad("truncated header"))? as usize;
    let phentsize = u16le(elf, 54).ok_or_else(|| bad("truncated header"))? as usize;
    let phnum = u16le(elf, 56).ok_or_else(|| bad("truncated header"))? as usize;
    for i in 0..phnum {
        let at = phoff + i * phentsize;
        let p_type = u32::from_le_bytes(
            elf.get(at..at + 4)
                .ok_or_else(|| bad("truncated phdr"))?
                .try_into()
                .unwrap(),
        );
        if p_type != 1 {
            continue; // PT_LOAD only
        }
        let p_offset = u64le(elf, at + 8).ok_or_else(|| bad("truncated phdr"))? as usize;
        let p_vaddr = u64le(elf, at + 16).ok_or_else(|| bad("truncated phdr"))?;
        let p_filesz = u64le(elf, at + 32).ok_or_else(|| bad("truncated phdr"))? as usize;
        let p_memsz = u64le(elf, at + 40).ok_or_else(|| bad("truncated phdr"))?;
        let file_bytes = elf
            .get(p_offset..p_offset + p_filesz)
            .ok_or_else(|| bad("PT_LOAD beyond file end"))?;
        mem.write_slice(file_bytes, GuestAddress(p_vaddr))
            .map_err(|e| BootError::Mem(format!("PT_LOAD copy: {e}")))?;
        // Explicit zero-fill of [filesz, memsz).
        let tail = p_memsz.saturating_sub(p_filesz as u64) as usize;
        if tail > 0 {
            mem.write_slice(&vec![0u8; tail], GuestAddress(p_vaddr + p_filesz as u64))
                .map_err(|e| BootError::Mem(format!("bss zero-fill: {e}")))?;
        }
    }
    Ok(entry)
}

/// Identity map [0, mem_bytes) with 2 MiB pages: PML4 -> PDPT -> one PD.
fn write_page_tables(
    mem: &impl Bytes<GuestAddress, E = vm_memory::GuestMemoryError>,
    mem_bytes: u64,
) -> Result<(), BootError> {
    let w = |gpa: u64, v: u64| {
        mem.write_slice(&v.to_le_bytes(), GuestAddress(gpa))
            .map_err(|e| BootError::Mem(format!("page table write: {e}")))
    };
    w(PML4_GPA, PDPT_GPA | 0b11)?; // present | writable
    w(PDPT_GPA, PD_GPA | 0b11)?;
    let pages = mem_bytes.div_ceil(2 << 20);
    for i in 0..pages {
        // present | writable | PS (2 MiB page)
        w(PD_GPA + i * 8, (i * (2 << 20)) | 0b1000_0011)?;
    }
    Ok(())
}

/// The canonical BootInfo page (nanokernel src/lib.rs owns the layout).
fn write_bootinfo(
    mem: &impl Bytes<GuestAddress, E = vm_memory::GuestMemoryError>,
    mem_bytes: u64,
    cmdline: &[u8],
) -> Result<(), BootError> {
    let mut page = Vec::with_capacity(0x20 + cmdline.len());
    page.extend_from_slice(b"DHBI");
    page.extend_from_slice(&1u32.to_le_bytes());
    page.extend_from_slice(&mem_bytes.to_le_bytes());
    page.extend_from_slice(&0xD000_0000u64.to_le_bytes());
    page.extend_from_slice(&(cmdline.len() as u32).to_le_bytes());
    page.extend_from_slice(&0u32.to_le_bytes());
    page.extend_from_slice(cmdline);
    mem.write_slice(&page, GuestAddress(BOOTINFO_GPA))
        .map_err(|e| BootError::Mem(format!("bootinfo write: {e}")))
}

/// Direct 64-bit entry (ARCH §2.3): CR0/CR3/CR4/EFER and the segment
/// caches via KVM_SET_SREGS — no real-mode phase, no in-memory GDT (KVM
/// honors the cached descriptors).
fn enter_long_mode(vcpu: &kvm_ioctls::VcpuFd, entry: u64) -> Result<(), BootError> {
    let kvm_err = |e: kvm_ioctls::Error| BootError::Kvm(e.to_string());
    let mut sregs = vcpu.get_sregs().map_err(kvm_err)?;

    let code = kvm_segment {
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
    let data = kvm_segment {
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

fn run_until_hlt(mut slot: dh_vmm::kvm::SlotVm, max_exits: u64) -> Result<BootOutcome, BootError> {
    let mut serial = Vec::new();
    let mut exits = 0u64;
    while exits < max_exits {
        exits += 1;
        let exit = slot
            .vcpu
            .run()
            .map_err(|e| BootError::Kvm(format!("KVM_RUN: {e}")))?;
        match exit {
            // INs answered on the raw exit: the kvm_run buffer must be
            // written before re-entry (classify_exit IN-FILL contract).
            // M0 models every IN in the serial + detcall windows as zeros.
            VcpuExit::IoIn(_port, data) => data.fill(0),
            VcpuExit::IoOut(port, data) if (SERIAL_BASE..SERIAL_END).contains(&port) => {
                serial.extend_from_slice(data);
            }
            other => match classify_exit(other) {
                ExitEvent::Hlt => return Ok(BootOutcome { serial, exits }),
                ExitEvent::DetcallOut { .. } | ExitEvent::PioIgnored { .. } => {}
                ExitEvent::MmioRead { gpa, .. } | ExitEvent::MmioWrite { gpa, .. } => {
                    return Err(BootError::UnexpectedExit(format!(
                        "MMIO at {gpa:#x} (M0 loop has no device bus)"
                    )));
                }
                ev => {
                    return Err(BootError::UnexpectedExit(format!("{ev:?}")));
                }
            },
        }
    }
    Err(BootError::ExitBudget)
}
