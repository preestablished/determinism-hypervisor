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

fn assert_guest_shape(name: &str, elf: &[u8]) {
    assert_eq!(&elf[0..4], b"\x7fELF", "{name}");
    assert_eq!(elf[4], 2, "{name}: ELFCLASS64");
    assert_eq!(elf[5], 1, "{name}: little-endian");
    assert_eq!(u16le(elf, 16), 2, "{name}: ET_EXEC (static, no PIE)");
    assert_eq!(u16le(elf, 18), 62, "{name}: EM_X86_64");
    assert_eq!(
        u64le(elf, 24),
        NANOKERNEL_LOAD_ADDR,
        "{name}: e_entry == load addr (crt0 .text.start placed first)"
    );

    // At least one PT_LOAD covering the entry address.
    let phoff = u64le(elf, 32) as usize;
    let phentsize = u16le(elf, 54) as usize;
    let phnum = u16le(elf, 56) as usize;
    assert!(phnum >= 1, "{name}");
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
    assert!(covers_entry, "{name}: a PT_LOAD must cover the entry point");

    // "tiny freestanding" (ARCH §1: ~2 KiB of program): the ELF with
    // headers and bss-free file image stays comfortably small.
    assert!(
        elf.len() < 64 * 1024,
        "{name}: nanokernel ELF unexpectedly large: {} bytes",
        elf.len()
    );
}

#[test]
fn every_guest_is_a_static_x86_64_exec_at_the_load_addr() {
    assert_guest_shape("pipeline_smoke", pipeline_smoke_elf());
    assert_guest_shape("landing_loop", landing_loop_elf());
    assert_guest_shape("device_exercise", device_exercise_elf());
    assert_guest_shape("hello", hello_elf());
    assert_guest_shape("sti_window", sti_window_elf());
    assert_guest_shape("timer_guest", timer_guest_elf());
    assert_guest_shape("counting", counting_elf());
    assert_guest_shape("rep_loop", rep_loop_elf());
    assert_guest_shape("sse_probe", sse_probe_elf());
    assert_guest_shape("pad_echo", pad_echo_elf());
}

/// include/bootinfo.inc is the asm side of the ABI — parse its %defines
/// and compare against the Rust constants so the two cannot drift.
#[test]
fn bootinfo_inc_matches_rust_constants() {
    let inc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/include/bootinfo.inc"))
        .unwrap();
    // Exact-token match: `%define NAME VALUE` split on whitespace, never a
    // substring search (BOOTINFO_OFF_CMDLINE must not match the _LEN line).
    let lookup = |name: &str| -> u64 {
        let line = inc
            .lines()
            .find(|l| {
                let mut t = l.split_whitespace();
                t.next() == Some("%define") && t.next() == Some(name)
            })
            .unwrap_or_else(|| panic!("missing %define {name}"));
        let val = line.split_whitespace().nth(2).unwrap();
        if let Some(hex) = val.strip_prefix("0x") {
            u64::from_str_radix(hex, 16).unwrap()
        } else {
            val.parse().unwrap()
        }
    };
    assert_eq!(lookup("BOOTINFO_MAGIC"), u64::from(BOOTINFO_MAGIC));
    assert_eq!(lookup("BOOTINFO_VERSION"), u64::from(BOOTINFO_VERSION));
    assert_eq!(lookup("BOOTINFO_OFF_MAGIC"), BOOTINFO_OFF_MAGIC as u64);
    assert_eq!(lookup("BOOTINFO_OFF_VERSION"), BOOTINFO_OFF_VERSION as u64);
    assert_eq!(
        lookup("BOOTINFO_OFF_MEM_SIZE"),
        BOOTINFO_OFF_MEM_SIZE as u64
    );
    assert_eq!(
        lookup("BOOTINFO_OFF_MMIO_BASE"),
        BOOTINFO_OFF_MMIO_BASE as u64
    );
    assert_eq!(
        lookup("BOOTINFO_OFF_CMDLINE_LEN"),
        BOOTINFO_OFF_CMDLINE_LEN as u64
    );
    assert_eq!(lookup("BOOTINFO_OFF_CMDLINE"), BOOTINFO_OFF_CMDLINE as u64);
}

/// Pin the landing loop's harness-facing constants against the asm source
/// (same drift-protection idea as the bootinfo test): the loop body between
/// `.loop:` and its closing `jnz .loop` must be exactly
/// LANDING_LOOP_INSTRS_PER_ITER instructions, and DEFAULT_ITERS must match.
#[test]
fn landing_loop_asm_matches_rust_constants() {
    let asm = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/asm/landing_loop.asm"))
        .unwrap();

    // %define DEFAULT_ITERS <n>
    let default_iters: u64 = asm
        .lines()
        .find_map(|l| {
            let mut t = l.split_whitespace();
            (t.next() == Some("%define") && t.next() == Some("DEFAULT_ITERS"))
                .then(|| t.next().unwrap().parse().unwrap())
        })
        .expect("missing %define DEFAULT_ITERS");
    assert_eq!(default_iters, LANDING_LOOP_DEFAULT_ITERS);

    // Count instruction lines between `.loop:` and the `jnz .loop`
    // (inclusive): non-empty, non-comment, non-label, non-directive.
    let mut in_loop = false;
    let mut instrs = 0u64;
    for line in asm.lines() {
        let t = line.split(';').next().unwrap().trim();
        if t == ".loop:" {
            in_loop = true;
            continue;
        }
        if !in_loop || t.is_empty() || t.ends_with(':') || t.starts_with("align") {
            continue;
        }
        instrs += 1;
        if t.starts_with("jnz") {
            break;
        }
    }
    assert_eq!(
        instrs, LANDING_LOOP_INSTRS_PER_ITER,
        "loop body drifted from the documented per-iteration count"
    );
}

/// Same pin for rep_loop (bead 8g1): the body between `.loop:` and its
/// closing `jmp .loop` must be exactly REP_LOOP_INSTRS_PER_ITER
/// instructions (REP MOVSB is one line, one retirement), and the
/// `mov rcx, 64` must match REP_LOOP_RCX_AT_REP_START — the landing
/// test's mid-REP detector depends on both.
#[test]
fn rep_loop_asm_matches_rust_constants() {
    let asm =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/asm/rep_loop.asm")).unwrap();
    let mut in_loop = false;
    let mut instrs = 0u64;
    let mut rcx_load: Option<u64> = None;
    for line in asm.lines() {
        let t = line.split(';').next().unwrap().trim();
        if t == ".loop:" {
            in_loop = true;
            continue;
        }
        if !in_loop || t.is_empty() || t.ends_with(':') || t.starts_with("align") {
            continue;
        }
        instrs += 1;
        if let Some(rest) = t.strip_prefix("mov") {
            let rest = rest.trim();
            if let Some(v) = rest.strip_prefix("rcx,") {
                rcx_load = Some(v.trim().parse().unwrap());
            }
        }
        if t.starts_with("jmp") {
            break;
        }
    }
    assert_eq!(
        instrs, REP_LOOP_INSTRS_PER_ITER,
        "rep_loop body drifted from the documented per-iteration count"
    );
    assert_eq!(
        rcx_load,
        Some(REP_LOOP_RCX_AT_REP_START),
        "the mid-REP detector value drifted from the asm"
    );
}

/// timer_guest's TABLE_GPA %define must match the Rust constant.
#[test]
fn timer_guest_table_gpa_matches() {
    let asm = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/asm/timer_guest.asm"))
        .unwrap();
    let v = asm
        .lines()
        .find_map(|l| {
            let mut t = l.split_whitespace();
            (t.next() == Some("%define") && t.next() == Some("TABLE_GPA"))
                .then(|| t.next().unwrap())
        })
        .expect("missing %define TABLE_GPA");
    let parsed = u64::from_str_radix(v.trim_start_matches("0x"), 16).unwrap();
    assert_eq!(parsed, TIMER_GUEST_TABLE_GPA);
}

/// hello's emitted bytes live in .rodata — the ELF must literally contain
/// HELLO_SERIAL_OUTPUT, so the constant and the asm string cannot drift.
#[test]
fn hello_elf_embeds_the_serial_string() {
    let elf = hello_elf();
    let needle = HELLO_SERIAL_OUTPUT;
    assert!(
        elf.windows(needle.len()).any(|w| w == needle),
        "HELLO_SERIAL_OUTPUT not found in hello.elf"
    );
}

/// Same drift pin for pad_echo (bead 29a): the table GPA and the fixed
/// frame pacing the M5 acceptance schedules against.
#[test]
fn pad_echo_asm_matches_rust_constants() {
    let asm =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/asm/pad_echo.asm")).unwrap();
    let define = |name: &str| -> u64 {
        asm.lines()
            .find_map(|l| {
                let mut t = l.split_whitespace();
                (t.next() == Some("%define") && t.next() == Some(name)).then(|| {
                    let v = t.next().unwrap();
                    if let Some(hex) = v.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16).unwrap()
                    } else {
                        v.parse().unwrap()
                    }
                })
            })
            .unwrap_or_else(|| panic!("missing %define {name}"))
    };
    assert_eq!(define("TABLE_GPA"), PAD_ECHO_TABLE_GPA);
    assert_eq!(define("TABLE_MASK"), PAD_ECHO_TABLE_CAPACITY - 1);
    assert_eq!(define("PACE_ITERS"), PAD_ECHO_PACE_ITERS);
    // Register offsets pinned against the DEVICE-SIDE truth, not
    // re-typed literals (iteration-81 review I2).
    assert_eq!(define("PAD_BASE"), dh_devices::pad::PV_PAD_BASE);
    assert_eq!(define("REG_PAD0"), dh_devices::pad::REG_PAD0);
    assert_eq!(define("REG_FRAME"), dh_devices::pad::REG_FRAME_COUNTER);
    assert_eq!(define("SERIAL_PORT"), 0x3F8); // dh_vmm kvm::PIO_SERIAL_BASE (x86-gated module)

    // Header + ring must end strictly below the device_exercise channel
    // (const-evaluated so the clippy lane sees it too).
    const _TABLE_FITS: () = assert!(
        PAD_ECHO_TABLE_GPA + 8 + PAD_ECHO_TABLE_CAPACITY * PAD_ECHO_ENTRY_BYTES
            <= DEVICE_EXERCISE_CHANNEL_GPA
    );

    // Pace-loop body pin (same idea as landing_loop): 7 instructions
    // between `.pace:` and its `jnz` inclusive — the M5 icount baseline
    // must not silently drift.
    let mut in_loop = false;
    let mut instrs = 0u64;
    for line in asm.lines() {
        let t = line.split(';').next().unwrap().trim();
        if t == ".pace:" {
            in_loop = true;
            continue;
        }
        if !in_loop || t.is_empty() || t.ends_with(':') || t.starts_with("align") {
            continue;
        }
        instrs += 1;
        if t.starts_with("jnz") {
            break;
        }
    }
    assert_eq!(instrs, 7, "pace-loop body drifted");
}
