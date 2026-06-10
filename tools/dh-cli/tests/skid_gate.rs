//! The bead-19l exit gate, live (kvm-intel lane + lab box): measured max
//! PMI skid must stay under skid_margin / 2.

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
fn measured_skid_stays_under_half_margin() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let r = dh_cli::skid::measure(50).expect("measurement must complete");
    assert_eq!(r.histogram.samples(), 50);
    r.gate.expect("R1 gate: max skid must be < skid_margin/2");
    // Sanity on this silicon class: skid is a few dozen instructions.
    assert!(r.histogram.max().unwrap() < 200, "skid order-of-magnitude");
}

#[test]
fn phase1_gate_smoke() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    // The operator command itself, exercised in CI at a smoke count.
    let (plain, timer) = dh_cli::gate::run_gate(2).expect("gate must run");
    assert!(plain.passed(), "{}", plain.artifact());
    assert!(timer.passed(), "{}", timer.artifact());
}
