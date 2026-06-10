//! M2 counting-semantics acceptance (bead gfb; the R2 empirics test):
//! single-step the counting guest's exactly-1,000-instruction region
//! and attribute EVERY §3.1 retirement case individually:
//!
//!   plain instructions     — +1 each, RIP advances (996 of them);
//!   REP MOVSB (RCX=256)    — traps once per iteration with RIP frozen
//!                            and the count UNCHANGED, then +1 exactly
//!                            once when RIP advances (retires as ONE);
//!   CPUID                  — kernel-emulated: RIP advances with NO
//!                            userspace exit and +0 (the unique
//!                            no-exit/advance/zero signature);
//!   MMIO read + write      — exit to userspace, +0 each;
//!   the S→E marker window  — counter delta exactly 997
//!                            (COUNTING_DELTA_AT_OUT_EXITS), measured
//!                            UNDER single-step: stepping must not
//!                            change retirement;
//!   HLT                    — the previously-unmeasured empiric: plain
//!                            Hlt-exit cycles in crt0's hlt/jmp park
//!                            loop must advance the counter by exactly
//!                            1 per cycle (the jmp; HLT itself retires
//!                            ZERO like every other exiting
//!                            instruction).
//!
//! The full attribution trace is then re-derived from a second cold
//! boot and must match bit-identically. A failure here on a new host
//! kernel/microcode is the R2 alarm: trigger the documented
//! BR_INST_RETIRED fallback decision — do not patch around it.
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

// Everything here drives KVM (x86_64-only; bead v5w) - the whole test
// target compiles to empty on other arches.
#![cfg(target_arch = "x86_64")]

use std::io::ErrorKind;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_vmm::boundary::{step_one_entry, BoundaryError};
use dh_vmm::kvm::{KvmSystem, SlotVm};
use kvm_ioctls::VcpuExit;

const STEP_CAP: usize = 5_000;
/// on_exit ends the step at an MMIO write (see the sentinel match).
const MMIO_WRITE_SENTINEL: &str = "mmio-write-ends-step";

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

/// What happened during one single-step entry.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    /// Counter delta across the entry.
    delta: u64,
    /// RIP at the trap.
    rip: u64,
    /// Exits the entry produced, classified.
    exits: Vec<ExitKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExitKind {
    SerialOut(u8),
    MmioRead(u64),
    MmioWrite(u64, u8),
}

struct Trace {
    steps: Vec<Step>,
    /// Counter values captured INSIDE the marker OUT exits.
    at_s: u64,
    at_e: u64,
    /// (deltas across 10 consecutive plain Hlt-exit park cycles).
    hlt_cycle_deltas: Vec<u64>,
}

fn boot(elf: &[u8]) -> Result<(SlotVm, InstRetired), String> {
    dh_vmm::run::install_kick_handler().map_err(|e| format!("kick: {e}"))?;
    let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
    let slot = sys.create_slot_vm(16 << 20).map_err(|e| format!("{e:?}"))?;
    dh_vmm::boot::load_and_enter(&slot, elf, b"").map_err(|e| format!("{e}"))?;
    let counter = InstRetired::open_for_current_thread().map_err(|e| format!("{e:?}"))?;
    counter
        .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
        .map_err(|e| format!("{e:?}"))?;
    counter
        .arm_period(NEVER_FIRES_PERIOD)
        .map_err(|e| format!("{e:?}"))?;
    counter.reset().map_err(|e| format!("{e:?}"))?;
    counter.enable().map_err(|e| format!("{e:?}"))?;
    Ok((slot, counter))
}

/// Single-step from boot through the E marker plus the closing `ret`,
/// recording the full attribution trace, then measure HLT retirement
/// with plain Hlt-exit park cycles.
fn trace_counting() -> Result<Trace, String> {
    let (mut slot, counter) = boot(nanokernel::counting_elf())?;
    let mut steps: Vec<Step> = Vec::new();
    let mut at_s = None;
    let mut at_e = None;
    let mut prev_count = 0u64;
    let mut seen_e_steps = 0usize;
    for _ in 0..STEP_CAP {
        let mut exits = Vec::new();
        let counter_ref = &counter;
        let at_s_ref = &mut at_s;
        let at_e_ref = &mut at_e;
        let exits_ref = &mut exits;
        let mut on_exit = |exit: VcpuExit| match exit {
            VcpuExit::IoOut(0x3F8, data) => {
                let now = counter_ref
                    .read()
                    .map_err(|e| BoundaryError::Exit(format!("counter at marker: {e:?}")))?;
                for &b in data.iter() {
                    if b == nanokernel::COUNTING_MARK_START {
                        *at_s_ref = Some(now);
                    } else if b == nanokernel::COUNTING_MARK_END {
                        *at_e_ref = Some(now);
                    }
                    exits_ref.push(ExitKind::SerialOut(b));
                }
                Ok(())
            }
            VcpuExit::MmioRead(gpa, data) => {
                data.fill(0);
                exits_ref.push(ExitKind::MmioRead(gpa));
                Ok(())
            }
            VcpuExit::MmioWrite(gpa, data) => {
                exits_ref.push(ExitKind::MmioWrite(gpa, data.first().copied().unwrap_or(0)));
                Err(BoundaryError::Exit(MMIO_WRITE_SENTINEL.to_string()))
            }
            other => Err(BoundaryError::Exit(format!("unexpected exit: {other:?}"))),
        };
        let b = match step_one_entry(&mut slot.vcpu, &counter, &mut on_exit) {
            Ok(b) => b,
            // MMIO-WRITE SENTINEL (measured, kernel 6.8): the write exit
            // eats the single-step trap (the emulator completes the
            // instruction and clears TF without the #DB), so on_exit
            // ends the step there and we synthesize the boundary from
            // the registers — the instruction HAS completed (RIP
            // advanced, zero retirement), keeping the attribution
            // strictly per-instruction. boundary.rs re-arms TF for the
            // engine's own loops; the next step here re-arms naturally.
            Err(BoundaryError::Exit(msg)) if msg == MMIO_WRITE_SENTINEL => {
                let regs = slot
                    .vcpu
                    .get_regs()
                    .map_err(|e| format!("regs after MMIO write: {e}"))?;
                dh_vmm::boundary::Boundary {
                    icount: counter.read().map_err(|e| format!("{e:?}"))?,
                    rip: regs.rip,
                    rcx: regs.rcx,
                }
            }
            Err(e) => return Err(format!("step {}: {e}", steps.len())),
        };
        steps.push(Step {
            delta: b.icount - prev_count,
            rip: b.rip,
            exits,
        });
        prev_count = b.icount;
        if at_e.is_some() {
            seen_e_steps += 1;
            // One more step after the E-OUT step: the closing `ret`.
            // The next instruction is crt0's park `hlt` — measured below
            // WITHOUT single-step (step_one_entry's Hlt precondition).
            if seen_e_steps >= 2 {
                break;
            }
        }
    }
    let at_s = at_s.ok_or("S marker never seen")?;
    let at_e = at_e.ok_or("E marker never seen")?;

    // ---- the HLT empiric: plain runs in the hlt/jmp park loop ----
    // Each cycle: the parked `hlt` exits (not yet retired), resumes,
    // completes, `jmp .park` retires (+1), `hlt` exits again. A
    // per-cycle delta of exactly 1 means HLT retires ZERO.
    let mut hlt_cycle_deltas = Vec::new();
    let mut last = counter.read().map_err(|e| format!("{e:?}"))?;
    for i in 0..11 {
        match slot.vcpu.run() {
            Ok(VcpuExit::Hlt) => {
                let now = counter.read().map_err(|e| format!("{e:?}"))?;
                if i > 0 {
                    hlt_cycle_deltas.push(now - last);
                }
                last = now;
            }
            Ok(other) => return Err(format!("park: unexpected exit {other:?}")),
            Err(e) => return Err(format!("park KVM_RUN: {e}")),
        }
    }
    Ok(Trace {
        steps,
        at_s,
        at_e,
        hlt_cycle_deltas,
    })
}

/// Regression for the iteration-50 engine fix: a near-approach landing
/// whose window crosses the MMIO-WRITE instruction. The write exit eats
/// the single-step trap (emulator clears TF without the #DB); without
/// boundary.rs re-arming guest_debug after handled exits, the vCPU
/// free-runs ~700 instructions to the park HLT and the landing dies
/// with a loud Overshoot.
#[test]
fn landing_across_an_mmio_write_does_not_free_run() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (mut slot, counter) = boot(nanokernel::counting_elf()).expect("boot");
    // The THR-mirror write completes at icount 12 (see the attribution
    // trace); 20 sits in the filler run just past it, so the
    // near-approach window [12, 20] spans the hazard.
    let b = dh_vmm::boundary::land_at(
        &mut slot.vcpu,
        &counter,
        20,
        &dh_vmm::boundary::Margins {
            skid_margin: 8,
            resync_slack: 8,
        },
        &mut |exit: VcpuExit| match exit {
            VcpuExit::IoOut(0x3F8, _) => Ok(()),
            VcpuExit::MmioRead(_, data) => {
                data.fill(0);
                Ok(())
            }
            VcpuExit::MmioWrite(..) => Ok(()),
            other => Err(BoundaryError::Exit(format!("unexpected exit: {other:?}"))),
        },
    )
    .expect("landing across the MMIO write");
    assert_eq!(b.icount, 20, "exact landing despite the eaten trap");
}

#[test]
fn single_step_attribution_of_every_retirement_case() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let t = trace_counting().expect("attribution trace");

    // The S→E window under single-step: stepping must not change
    // retirement — same 997 the free-running smoke measures.
    assert_eq!(
        t.at_e - t.at_s,
        nanokernel::COUNTING_DELTA_AT_OUT_EXITS,
        "S->E window under single-step"
    );

    // Locate the marker steps.
    let s_idx = t
        .steps
        .iter()
        .position(|s| {
            s.exits
                .contains(&ExitKind::SerialOut(nanokernel::COUNTING_MARK_START))
        })
        .unwrap();
    let e_idx = t
        .steps
        .iter()
        .position(|s| {
            s.exits
                .contains(&ExitKind::SerialOut(nanokernel::COUNTING_MARK_END))
        })
        .unwrap();
    // Marker OUTs themselves retire zero.
    assert_eq!(t.steps[s_idx].delta, 0, "S OUT retires zero");
    assert_eq!(t.steps[e_idx].delta, 0, "E OUT retires zero");

    let region = &t.steps[s_idx + 1..e_idx];

    // ---- per-class attribution over the region ----
    let mut plain = 0u64;
    let mut cpuid = 0u64;
    let mut mmio_reads = 0u64;
    let mut mmio_writes = 0u64;
    let mut rep_frozen = 0u64;
    let mut rep_retire = 0u64;
    let mut prev_rip = t.steps[s_idx].rip;
    let mut prev_was_frozen = false;
    for (i, s) in region.iter().enumerate() {
        let advanced = s.rip != prev_rip;
        let frozen = !advanced && s.delta == 0;
        match (&s.exits[..], advanced, s.delta) {
            ([], false, 0) => rep_frozen += 1, // REP iterating, RIP frozen
            ([], true, 1) => {
                // Plain instruction — or the REP's completing advance,
                // distinguished by the PREVIOUS step having been a
                // frozen-RIP REP iteration.
                if prev_was_frozen {
                    rep_retire += 1;
                } else {
                    plain += 1;
                }
            }
            ([], true, 0) => cpuid += 1, // kernel-emulated: no exit, +0
            ([ExitKind::MmioRead(gpa)], true, 0) => {
                assert_eq!(*gpa, 0xD000_0008, "the pv-clock VNS read");
                mmio_reads += 1;
            }
            ([ExitKind::MmioWrite(gpa, byte)], true, 0) => {
                assert_eq!(
                    (*gpa, *byte),
                    (
                        nanokernel::COUNTING_MMIO_THR_GPA,
                        nanokernel::COUNTING_MMIO_THR_BYTE
                    ),
                    "the serial THR mirror write"
                );
                mmio_writes += 1;
            }
            other => panic!("unclassifiable step {i}: {other:?} (rip {:#x})", s.rip),
        }
        prev_was_frozen = frozen;
        prev_rip = s.rip;
    }

    // The region's composition, attributed case by case (ARCH §3.1):
    // 996 plain (+1 each) + REP retiring once + CPUID/MMIO zero.
    assert_eq!(plain, 996, "plain instructions");
    assert_eq!(cpuid, 1, "exactly one kernel-emulated CPUID");
    assert_eq!(mmio_reads, 1, "exactly one MMIO read");
    assert_eq!(mmio_writes, 1, "exactly one MMIO write");
    assert_eq!(rep_retire, 1, "REP MOVSB retires exactly ONCE");
    assert_eq!(
        rep_frozen,
        nanokernel::COUNTING_REP_BYTES - 1,
        "one frozen trap per REP iteration except the completing one"
    );
    let retired: u64 = region.iter().map(|s| s.delta).sum();
    assert_eq!(retired, 997, "attributed retirements sum to the window");

    // ---- HLT: the previously-unmeasured empiric ----
    // Every park cycle is hlt (exits, retires ZERO) + jmp (+1).
    assert_eq!(
        t.hlt_cycle_deltas,
        vec![1; 10],
        "HLT must retire zero: each park cycle advances by the jmp alone"
    );

    // ---- the whole trace replays bit-identically from a cold boot ----
    let t2 = trace_counting().expect("second trace");
    assert_eq!(t.steps, t2.steps, "attribution trace must replay");
    assert_eq!(
        (t.at_s, t.at_e, t.hlt_cycle_deltas),
        (t2.at_s, t2.at_e, t2.hlt_cycle_deltas),
        "marker window and HLT cycles must replay"
    );
}
