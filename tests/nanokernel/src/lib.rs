//! Nanokernel test guests (ARCH §1): the built ELF images plus the BootInfo
//! ABI (ARCH §2.3) shared between dh-vmm's loader and the guests' crt0.
//!
//! The asm side of the ABI lives in include/bootinfo.inc; the integration
//! test parses that file and fails on any drift from these constants.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_spells_dhbi() {
        assert_eq!(BOOTINFO_MAGIC, 0x4942_4844);
        assert_eq!(&BOOTINFO_MAGIC.to_le_bytes(), b"DHBI");
    }

    #[test]
    fn elf_is_embedded_and_nonempty() {
        assert!(!pipeline_smoke_elf().is_empty());
    }
}
