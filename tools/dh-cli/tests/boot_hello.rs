//! The M0 acceptance, LIVE (kvm-intel lane + lab box; skips elsewhere):
//! boot hello.elf, read "HELLO\n" off the serial sink, exit on HLT.

use std::io::ErrorKind;

fn kvm_usable() -> bool {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
    {
        Ok(_) => true,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) => false,
        Err(e) => panic!("unexpected /dev/kvm probe failure: {e}"),
    }
}

#[test]
fn hello_boots_and_prints() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let out = dh_cli::boot::boot(nanokernel::hello_elf(), 16 << 20, b"", 10_000)
        .expect("hello must boot to HLT");
    assert_eq!(out.serial, nanokernel::HELLO_SERIAL_OUTPUT);
}

#[test]
fn pipeline_smoke_reports_bootinfo_ok() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let out = dh_cli::boot::boot(nanokernel::pipeline_smoke_elf(), 16 << 20, b"", 10_000)
        .expect("pipeline_smoke must boot to HLT");
    assert_eq!(out.serial, b"K", "BootInfo magic/version must check out");
}

#[test]
fn landing_loop_is_deterministic_across_runs() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let run = |cmdline: &[u8]| {
        let out = dh_cli::boot::boot(nanokernel::landing_loop_elf(), 16 << 20, cmdline, 10_000)
            .expect("landing loop must boot to HLT");
        (out.serial, out.exits)
    };
    // Same cmdline -> identical observable outcome, twice; scaled run too.
    assert_eq!(run(b""), run(b""));
    assert_eq!(run(b"1000"), run(b"1000"));
    assert_eq!(run(b"").0, b"L");
}
