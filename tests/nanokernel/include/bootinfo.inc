; BootInfo ABI (ARCH §2.3): the versioned struct dh-vmm places at a fixed
; GPA before entering the guest with RSI -> BootInfo. Mirrored by the Rust
; constants in src/lib.rs — the elf_shape integration test fails on drift.
;
;   0x00  u32  magic        = "DHBI" (LE bytes 'D','H','B','I')
;   0x04  u32  version      = 1
;   0x08  u64  mem_size     guest RAM bytes
;   0x10  u64  mmio_base    device window base (0xD000_0000)
;   0x18  u32  cmdline_len  bytes at BOOTINFO_CMDLINE
;   0x1C  u32  reserved     0
;   0x20  u8[] cmdline      cmdline_len bytes, NOT NUL-terminated

%define BOOTINFO_MAGIC      0x49424844
%define BOOTINFO_VERSION    1

%define BOOTINFO_OFF_MAGIC        0x00
%define BOOTINFO_OFF_VERSION      0x04
%define BOOTINFO_OFF_MEM_SIZE     0x08
%define BOOTINFO_OFF_MMIO_BASE    0x10
%define BOOTINFO_OFF_CMDLINE_LEN  0x18
%define BOOTINFO_OFF_CMDLINE      0x20
