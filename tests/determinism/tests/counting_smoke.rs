//! Live smoke for the counting guest (bead d34): boot it, serve its
//! exits raw, and assert the §3.1 marker-window arithmetic — counter
//! delta between the S-OUT and E-OUT exit moments is EXACTLY
//! COUNTING_DELTA_AT_OUT_EXITS (997: the 1,000-instruction region minus
//! its three VM-exiting instructions, which retire zero — the measured
//! empirics on this class), twice, identically.
//!
//! This is NOT the M2 counting_semantics acceptance (that single-steps
//! the region and attributes every §3.1 case individually — bead gfb);
//! it is the end-to-end sanity proof that the sequence is well-formed
//! and the window math holds on this determinism class.
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

// Everything here drives KVM (x86_64-only; bead v5w) - the whole test
// target compiles to empty on other arches.
#![cfg(target_arch = "x86_64")]

use std::io::ErrorKind;
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::KvmSystem;
use dh_vmm::runctl::{run_segment, Segment, StopReason, Until};
use kvm_ioctls::VcpuExit;

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

/// One cold boot of the counting guest. Returns (serial bytes incl. the
/// MMIO THR mirror byte, counter at S-OUT exit, counter at E-OUT exit).
fn run_counting() -> Result<(Vec<u8>, u64, u64), String> {
    dh_vmm::run::install_kick_handler().map_err(|e| format!("kick: {e}"))?;
    let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
    let mut slot = sys.create_slot_vm(16 << 20).map_err(|e| format!("{e:?}"))?;
    dh_vmm::boot::load_and_enter(&slot, nanokernel::counting_elf(), b"")
        .map_err(|e| format!("{e}"))?;
    let counter = InstRetired::open_for_current_thread().map_err(|e| format!("{e:?}"))?;
    counter
        .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
        .map_err(|e| format!("{e:?}"))?;
    counter
        .arm_period(NEVER_FIRES_PERIOD)
        .map_err(|e| format!("{e:?}"))?;
    counter.reset().map_err(|e| format!("{e:?}"))?;
    counter.enable().map_err(|e| format!("{e:?}"))?;

    let config = MachineConfig::new(
        16 << 20,
        [7; 32],
        BootSpec::Elf {
            kernel_hash: [7; 32],
            cmdline: Vec::new(),
        },
    );
    let mut chain = StateHashChain::new(&[7; 32], &[7; 32]);
    let pause = AtomicBool::new(false);

    let mut serial = Vec::new();
    let mut at_s = None;
    let mut at_e = None;
    {
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
        };
        let counter_ref = &counter;
        let mut on_exit = |exit: VcpuExit| match exit {
            VcpuExit::IoOut(0x3F8, data) => {
                // Counter reads are stable here: the vCPU is out of
                // guest mode at the exit (§3.1). NOTE: assumes the
                // guest's single-byte `out dx, al` markers — a batched
                // marker (rep outsb) would alias both markers onto ONE
                // counter read and silently measure a zero window.
                let now = counter_ref
                    .read()
                    .map_err(|e| BoundaryError::Exit(format!("counter read at marker: {e:?}")))?;
                for &b in data.iter() {
                    if b == nanokernel::COUNTING_MARK_START {
                        at_s = Some(now);
                    } else if b == nanokernel::COUNTING_MARK_END {
                        at_e = Some(now);
                    }
                }
                serial.extend_from_slice(data);
                Ok(())
            }
            // The region's pv-clock VNS read: deterministic zeros. The
            // 997 retirement count is independent of the VALUE read —
            // the real device bus (nonzero monotone vns) is bead gfb's
            // path and must re-confirm retirement there, not here.
            VcpuExit::MmioRead(_gpa, data) => {
                data.fill(0);
                Ok(())
            }
            // The region's debug-serial THR mirror write: low byte is
            // the transmitted character (crates/dh-devices serial.rs).
            VcpuExit::MmioWrite(gpa, data) => {
                if gpa == nanokernel::COUNTING_MMIO_THR_GPA && !data.is_empty() {
                    serial.push(data[0]);
                }
                Ok(())
            }
            other => Err(BoundaryError::Exit(format!("unexpected exit: {other:?}"))),
        };
        let out = run_segment(
            &mut seg,
            Until::IcountBudget(1_000_000),
            &mut || false,
            &mut on_exit,
        )
        .map_err(|e| format!("{e}"))?;
        if out.reason != StopReason::GuestHalted {
            return Err(format!("expected GuestHalted, got {:?}", out.reason));
        }
    }
    Ok((
        serial,
        at_s.ok_or("S marker never seen")?,
        at_e.ok_or("E marker never seen")?,
    ))
}

#[test]
fn marker_window_is_exactly_the_region_minus_its_exiting_instructions() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (serial, s1, e1) = run_counting().expect("first run");
    assert_eq!(
        serial,
        vec![
            nanokernel::COUNTING_MARK_START,
            nanokernel::COUNTING_MMIO_THR_BYTE,
            nanokernel::COUNTING_MARK_END
        ],
        "marker/MMIO byte order"
    );
    assert_eq!(
        e1 - s1,
        nanokernel::COUNTING_DELTA_AT_OUT_EXITS,
        "S->E exit-moment delta: region minus its VM-exiting \
         instructions (measured §3.1 empirics — exiting instructions \
         retire zero; see nanokernel::COUNTING_DELTA_AT_OUT_EXITS)"
    );

    // Cold-boot again: BOTH endpoints must be bit-identical, not just
    // the delta — s drift would mean the crt0/prologue instruction
    // stream changed underneath the constants.
    let (_, s2, e2) = run_counting().expect("second run");
    assert_eq!(
        (s2, e2),
        (s1, e1),
        "endpoints must be identical across runs"
    );
}
