//! M2 landing-precision acceptance (bead 8g1; IMPLEMENTATION-PLAN M2
//! accept): land at 10,000 random targets inside the 100M-instruction
//! landing loop — every landing must satisfy `icount == N` exactly with
//! ZERO overshoots (BoundaryError::Overshoot fails the run loudly) —
//! plus a REP-string torture pass: 1,000 random targets in a guest
//! whose loop is built around a 64-byte REP MOVSB, where RCX itself
//! detects a mid-REP boundary (64 at a REP start, 0 elsewhere, anything
//! else = §3.2 violation). Both passes run TWICE from cold boot and the
//! full landed tuple sequence (icount, rip, rcx) must be bit-identical.
//!
//! MARGINS: §3.2 (and config.rs) make skid_margin/resync_slack
//! landing-only knobs — the landed boundary is guaranteed independent
//! of them, and they are EXCLUDED from the config preimage. The
//! production margin (8192) costs ~8192 single-steps per landing
//! (~0.3 s); 20k landings at that cost is ~90 minutes, so the bulk of
//! the targets land under tight margins (256 first boot / 128 second
//! boot — 4x the measured max skid of 31 even at 128, with the skid
//! gate alerting at margin/2), while a PRODUCTION-MARGIN PREFIX of the
//! very same targets lands at 8192 on the first boot. The cross-boot
//! tuple identity then proves the §3.2 margin-independence contract on
//! real targets in the same breath: prefix tuples landed at 8192 must
//! equal the second boot's landed at 128.
//!
//! Targets are derived from a fixed-seed SplitMix64 — deterministic by
//! construction, no host randomness on the test path.
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

// Everything here drives KVM (x86_64-only; bead v5w) - the whole test
// target compiles to empty on other arches.
#![cfg(target_arch = "x86_64")]

use std::io::ErrorKind;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_vmm::boundary::{land_at, Boundary, BoundaryError, Margins};
use dh_vmm::kvm::KvmSystem;
use kvm_ioctls::VcpuExit;

const LANDING_TARGETS: usize = 10_000;
const REP_TARGETS: usize = 1_000;
/// Landing-loop targets stay below this so the far-approach arm can
/// never run into the guest's completion serial/HLT (~100M + prologue).
const LANDING_CEILING: u64 = 99_000_000;
const REP_CEILING: u64 = 3_000_000;
const TARGET_FLOOR: u64 = 1_000;

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

/// SplitMix64: tiny, deterministic, fixed seed — no host randomness.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `n` distinct sorted targets in [TARGET_FLOOR, ceiling), fixed seed.
fn targets(seed: u64, n: usize, ceiling: u64) -> Vec<u64> {
    let mut s = seed;
    let mut set = std::collections::BTreeSet::new();
    while set.len() < n {
        set.insert(TARGET_FLOOR + splitmix64(&mut s) % (ceiling - TARGET_FLOOR));
    }
    set.into_iter().collect()
}

/// Boot `elf`, then land at every target in order; `margins_for(i)`
/// picks the landing knobs per target. Returns the landed tuples. Any
/// exit at all is a failure (both guests are pure compute in the landed
/// range); any Overshoot is the failure this test exists to rule out.
fn land_sequence(
    elf: &[u8],
    cmdline: &[u8],
    targets: &[u64],
    margins_for: &dyn Fn(usize) -> Margins,
) -> Result<Vec<Boundary>, String> {
    dh_vmm::run::install_kick_handler().map_err(|e| format!("kick: {e}"))?;
    let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
    let mut slot = sys.create_slot_vm(16 << 20).map_err(|e| format!("{e:?}"))?;
    dh_vmm::boot::load_and_enter(&slot, elf, cmdline).map_err(|e| format!("{e}"))?;
    let counter = InstRetired::open_for_current_thread().map_err(|e| format!("{e:?}"))?;
    counter
        .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
        .map_err(|e| format!("{e:?}"))?;
    counter
        .arm_period(NEVER_FIRES_PERIOD)
        .map_err(|e| format!("{e:?}"))?;
    counter.reset().map_err(|e| format!("{e:?}"))?;
    counter.enable().map_err(|e| format!("{e:?}"))?;

    let mut out = Vec::with_capacity(targets.len());
    for (i, &t) in targets.iter().enumerate() {
        let b = land_at(
            &mut slot.vcpu,
            &counter,
            t,
            &margins_for(i),
            &mut |exit: VcpuExit| Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}"))),
        )
        .map_err(|e| format!("target {t}: {e}"))?;
        out.push(b);
    }
    Ok(out)
}

/// First boot: production margins (8192) for the prefix, tight (256)
/// for the bulk. Second boot: uniformly 128. Identical tuples across
/// the boots = the §3.2 margin-independence contract, live.
const PRODUCTION_PREFIX: usize = 100;

fn pass_a_margins(i: usize) -> Margins {
    if i < PRODUCTION_PREFIX {
        Margins::default()
    } else {
        Margins {
            skid_margin: 256,
            resync_slack: 256,
        }
    }
}

fn pass_b_margins(_i: usize) -> Margins {
    Margins {
        skid_margin: 128,
        resync_slack: 128,
    }
}

#[test]
fn ten_thousand_random_targets_zero_overshoots() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let ts = targets(0x8617_2026_0610_0001, LANDING_TARGETS, LANDING_CEILING);
    let first = land_sequence(nanokernel::landing_loop_elf(), b"", &ts, &pass_a_margins)
        .expect("first pass");
    for (b, &t) in first.iter().zip(&ts) {
        assert_eq!(b.icount, t, "landed icount must equal the target exactly");
    }
    // Same targets, fresh boot, DIFFERENT margins: the full tuple
    // sequence must repeat bit-identically (RIP at the same instruction
    // start, same RCX) — determinism AND margin-independence at once.
    let second = land_sequence(nanokernel::landing_loop_elf(), b"", &ts, &pass_b_margins)
        .expect("second pass");
    assert_eq!(first, second, "landing tuples must replay bit-identically");
}

#[test]
fn rep_string_boundaries_never_land_mid_rep() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let ts = targets(0x8617_2026_0610_0002, REP_TARGETS, REP_CEILING);
    let first =
        land_sequence(nanokernel::rep_loop_elf(), b"", &ts, &pass_a_margins).expect("first pass");
    for (b, &t) in first.iter().zip(&ts) {
        assert_eq!(b.icount, t, "landed icount must equal the target exactly");
        // The guest makes RCX a mid-REP detector: 64 exactly at a REP
        // start, 0 everywhere else. Any other value means a boundary
        // was declared mid-REP — the §3.2 violation.
        assert!(
            b.rcx == nanokernel::REP_LOOP_RCX_AT_REP_START || b.rcx == 0,
            "mid-REP boundary at target {t}: rip={:#x} rcx={}",
            b.rip,
            b.rcx
        );
    }
    // REP-start landings must actually occur for this test to mean
    // anything (~1/6 of random targets).
    let rep_starts = first
        .iter()
        .filter(|b| b.rcx == nanokernel::REP_LOOP_RCX_AT_REP_START)
        .count();
    assert!(
        rep_starts > REP_TARGETS / 20,
        "too few REP-start landings ({rep_starts}) — torture coverage lost"
    );
    let second =
        land_sequence(nanokernel::rep_loop_elf(), b"", &ts, &pass_b_margins).expect("second pass");
    assert_eq!(first, second, "landing tuples must replay bit-identically");
}
