//! Shape-checks the built guests (HOST-RUNNABLE; running them is
//! HARDWARE-GATED) and pins the asm↔Rust BootInfo ABI against drift.

use nanokernel::*;

/// Minimal ELF64 header reads (offsets per the ELF spec; no deps).
fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}
fn u64le(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

#[test]
fn pipeline_smoke_is_a_static_x86_64_exec_at_the_load_addr() {
    let elf = pipeline_smoke_elf();
    assert_eq!(&elf[0..4], b"\x7fELF");
    assert_eq!(elf[4], 2, "ELFCLASS64");
    assert_eq!(elf[5], 1, "little-endian");
    assert_eq!(u16le(elf, 16), 2, "ET_EXEC (static, no PIE)");
    assert_eq!(u16le(elf, 18), 62, "EM_X86_64");
    assert_eq!(
        u64le(elf, 24),
        NANOKERNEL_LOAD_ADDR,
        "e_entry == load addr (crt0 .text.start placed first)"
    );

    // At least one PT_LOAD covering the entry address.
    let phoff = u64le(elf, 32) as usize;
    let phentsize = u16le(elf, 54) as usize;
    let phnum = u16le(elf, 56) as usize;
    assert!(phnum >= 1);
    let mut covers_entry = false;
    for i in 0..phnum {
        let at = phoff + i * phentsize;
        let p_type = u32::from_le_bytes(elf[at..at + 4].try_into().unwrap());
        if p_type != 1 {
            continue; // not PT_LOAD
        }
        let vaddr = u64le(elf, at + 16);
        let memsz = u64le(elf, at + 40);
        if (vaddr..vaddr + memsz).contains(&NANOKERNEL_LOAD_ADDR) {
            covers_entry = true;
        }
    }
    assert!(covers_entry, "a PT_LOAD must cover the entry point");

    // "tiny freestanding" (ARCH §1: ~2 KiB of program): the ELF with
    // headers and bss-free file image stays comfortably small.
    assert!(
        elf.len() < 64 * 1024,
        "nanokernel ELF unexpectedly large: {} bytes",
        elf.len()
    );
}

/// include/bootinfo.inc is the asm side of the ABI — parse its %defines
/// and compare against the Rust constants so the two cannot drift.
#[test]
fn bootinfo_inc_matches_rust_constants() {
    let inc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/include/bootinfo.inc"))
        .unwrap();
    let lookup = |name: &str| -> u64 {
        let line = inc
            .lines()
            .find(|l| l.starts_with("%define") && l.contains(name))
            .unwrap_or_else(|| panic!("missing %define {name}"));
        let val = line.split_whitespace().nth(2).unwrap();
        if let Some(hex) = val.strip_prefix("0x") {
            u64::from_str_radix(hex, 16).unwrap()
        } else {
            val.parse().unwrap()
        }
    };
    assert_eq!(lookup("BOOTINFO_MAGIC "), u64::from(BOOTINFO_MAGIC));
    assert_eq!(lookup("BOOTINFO_VERSION "), u64::from(BOOTINFO_VERSION));
    assert_eq!(lookup("BOOTINFO_OFF_MAGIC "), BOOTINFO_OFF_MAGIC as u64);
    assert_eq!(lookup("BOOTINFO_OFF_VERSION "), BOOTINFO_OFF_VERSION as u64);
    assert_eq!(
        lookup("BOOTINFO_OFF_MEM_SIZE "),
        BOOTINFO_OFF_MEM_SIZE as u64
    );
    assert_eq!(
        lookup("BOOTINFO_OFF_MMIO_BASE "),
        BOOTINFO_OFF_MMIO_BASE as u64
    );
    assert_eq!(
        lookup("BOOTINFO_OFF_CMDLINE_LEN "),
        BOOTINFO_OFF_CMDLINE_LEN as u64
    );
    assert_eq!(lookup("BOOTINFO_OFF_CMDLINE "), BOOTINFO_OFF_CMDLINE as u64);
}
