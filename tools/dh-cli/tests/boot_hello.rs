//! The M0 acceptance, LIVE (kvm-intel lane + lab box; skips elsewhere):
//! boot hello.elf, read "HELLO\n" off the serial sink, exit on HLT.

// Everything here drives KVM (x86_64-only; bead v5w) - the whole test
// target compiles to empty on other arches.
#![cfg(target_arch = "x86_64")]

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

#[test]
fn run_segment_serves_serial_ins_under_the_landing_loop() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    // hello polls LSR via IN before every byte, so this drives the
    // run-loop's serial-IN arm under the PMI/single-step machinery
    // (boot.rs's debug loop has its own arm; this covers run.rs's).
    let go = |paranoid_hash| {
        dh_cli::run::run(
            nanokernel::hello_elf(),
            16 << 20,
            b"",
            dh_vmm::runctl::Until::IcountBudget(1_000_000),
            paranoid_hash,
        )
        .expect("hello must run under run_segment")
    };
    let (a, b, audit) = (go(false), go(false), go(true));
    assert_eq!(a.reason, "guest_halted");
    assert_eq!(a.serial, nanokernel::HELLO_SERIAL_OUTPUT);
    assert_eq!(
        (a.icount, a.rip, a.vns, &a.state_hash, &a.serial),
        (b.icount, b.rip, b.vns, &b.state_hash, &b.serial),
        "serial INs at boundaries must not perturb determinism"
    );
    assert_eq!(
        (a.icount, a.rip, a.vns, &a.state_hash),
        (audit.icount, audit.rip, audit.vns, &audit.state_hash),
        "--paranoid-hash is behavior-preserving when epoch hashes are already on"
    );
}

#[test]
fn sse_probe_proves_osfxsr() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    // Without CR4.OSFXSR the guest's first movdqa #UDs into a triple
    // fault (no IDT) — a Shutdown error, never a serial byte.
    let out = dh_cli::boot::boot(nanokernel::sse_probe_elf(), 16 << 20, b"", 10_000)
        .expect("SSE guest must boot to HLT");
    assert_eq!(out.serial, b"V", "SSE2 vector arithmetic must verify");
}

#[test]
fn device_exercise_reaches_a_real_mmio_exit() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    // The M1 loader maps the MMIO hole, so the device guest's first
    // pv-clock read must surface as an MMIO exit at 0xD000_0008 — NOT a
    // triple fault. The full device run loop is the M1 acceptance bead.
    let err = dh_cli::boot::boot(nanokernel::device_exercise_elf(), 16 << 20, b"", 10_000)
        .expect_err("debug loop has no device bus");
    let msg = format!("{err}");
    assert!(
        msg.contains("MMIO at 0xd000_0008") || msg.contains("MMIO at 0xd0000008"),
        "expected the pv-clock VNS MMIO exit, got: {msg}"
    );
}
