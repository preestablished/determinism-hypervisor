//! Nanokernel test guests (ARCH §1): the built ELF images plus the BootInfo
//! ABI shared between dh-vmm's loader and the guests' crt0.
//!
//! THIS MODULE IS THE CANONICAL BootInfo BINARY LAYOUT. ARCH §2.3 names
//! the struct ("versioned, carrying mem_size, MMIO base, cmdline bytes")
//! but defines no offsets — the loader (bead s0p: PT_LOAD loader, identity
//! page tables, direct 64-bit entry) must consume these constants, not
//! restate them. The asm side lives in include/bootinfo.inc; an
//! integration test parses that file and fails on any drift.
//!
//! LOADER CONTRACT (bead s0p): nanokernel ELFs carry PT_LOAD segments with
//! p_memsz > p_filesz (.bss holds BOOT_INFO_PTR and the 16 KiB stack) —
//! the loader MUST zero-fill [filesz, memsz), or crt0's stack and the
//! guests' zeroed-.bss assumption are garbage.

/// BootInfo magic: "DHBI" as little-endian bytes.
pub const BOOTINFO_MAGIC: u32 = u32::from_le_bytes(*b"DHBI");
pub const BOOTINFO_VERSION: u32 = 1;

/// Field offsets within the BootInfo page (see include/bootinfo.inc).
pub const BOOTINFO_OFF_MAGIC: usize = 0x00;
pub const BOOTINFO_OFF_VERSION: usize = 0x04;
pub const BOOTINFO_OFF_MEM_SIZE: usize = 0x08;
pub const BOOTINFO_OFF_MMIO_BASE: usize = 0x10;
pub const BOOTINFO_OFF_CMDLINE_LEN: usize = 0x18;
pub const BOOTINFO_OFF_CMDLINE: usize = 0x20;

/// Guest load address (link.ld); e_entry of every nanokernel ELF.
pub const NANOKERNEL_LOAD_ADDR: u64 = 0x10_0000;

/// The pipeline-proof guest: checks BootInfo, reports 'K'/'B' on the debug
/// serial port, parks in HLT (see asm/pipeline_smoke.asm).
pub fn pipeline_smoke_elf() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/pipeline_smoke.elf"))
}

/// The M2/M3 long-runner (see asm/landing_loop.asm): an LCG loop touching
/// a 64 KiB ring buffer, 'L' on serial when done. Iteration count comes
/// from the BootInfo cmdline's leading ASCII decimal digits (no digits or
/// "0" → [`LANDING_LOOP_DEFAULT_ITERS`]).
pub fn landing_loop_elf() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/landing_loop.elf"))
}

/// Loop-body instructions per iteration — the harness computes expected
/// icounts as `8 * iters + prologue` (prologue/epilogue/crt0 are a few
/// dozen instructions; harnesses calibrate the exact offset once, it is
/// deterministic).
pub const LANDING_LOOP_INSTRS_PER_ITER: u64 = 8;

/// Iterations when the cmdline carries none: 12.5M × 8 = 100M loop
/// instructions (the M2 landing test budget).
pub const LANDING_LOOP_DEFAULT_ITERS: u64 = 12_500_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_spells_dhbi() {
        assert_eq!(BOOTINFO_MAGIC, 0x4942_4844);
        assert_eq!(&BOOTINFO_MAGIC.to_le_bytes(), b"DHBI");
    }

    #[test]
    fn elves_are_embedded_and_nonempty() {
        assert!(!pipeline_smoke_elf().is_empty());
        assert!(!landing_loop_elf().is_empty());
    }

    #[test]
    fn default_iters_hit_the_100m_budget() {
        assert_eq!(
            LANDING_LOOP_INSTRS_PER_ITER * LANDING_LOOP_DEFAULT_ITERS,
            100_000_000
        );
    }
}
