//! The boundary engine (ARCH §3.2): stop the vCPU at EXACTLY icount = N.
//!
//! Far approach: arm the PMI at (d − SKID_MARGIN) and KVM_RUN — the
//! overflow signal kicks the run out within the skid (empirics, iteration
//! 16: skid = 18 instructions on this box, zero variance over 40 runs).
//! Near approach: KVM_GUESTDBG single-step, re-reading the counter after
//! EVERY step — never assume +1:
//!
//! - a debug trap is NOT retirement: REP string instructions trap once per
//!   iteration with RIP unchanged and retire (count +1) only when RIP
//!   advances, so trusting only (counter, RIP-advance) IS the §3.2 REP
//!   rule — no boundary is ever declared mid-REP;
//! - an instruction exited mid-emulation (MMIO not completed) has not
//!   retired; the exit is serviced and the count is unchanged.
//!
//! Overshoot (c > N) is [`BoundaryError::Overshoot`] — fatal, DATA_LOSS
//! class, never absorbed (risk R1: it means SKID_MARGIN is too small for
//! this host). KVM_GUESTDBG_ENABLE is dropped the instant the boundary is
//! reached; TF is never guest-visible (risk R10).
//!
//! Margins are MachineConfig material: they must make landing POSSIBLE,
//! but the landed boundary is independent of them.
//!
//! PMI throttling hazard (iteration-16 empirics): re-arming small periods
//! in a tight loop trips perf_event_max_sample_rate. The engine arms at
//! most once per far approach and parks the period at NEVER_FIRES_PERIOD
//! before stepping, so a landing costs O(1) arms.

use dh_detclock::counter::{CounterError, InstRetired, NEVER_FIRES_PERIOD};
use kvm_bindings::kvm_guest_debug;
use kvm_ioctls::{VcpuExit, VcpuFd};

use crate::run::{clear_immediate_exit, KickGuard};

/// §3.2 margins (MachineConfig fields; defaults are the doc's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Margins {
    pub skid_margin: u64,
    pub resync_slack: u64,
}

impl Default for Margins {
    fn default() -> Self {
        Margins {
            skid_margin: 8192,
            resync_slack: 1024,
        }
    }
}

/// A landed boundary. `rcx` is DIAGNOSTICS ONLY (REP progress snapshot) —
/// the canonical boundary identity is (icount, rip).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boundary {
    pub icount: u64,
    pub rip: u64,
    pub rcx: u64,
}

#[derive(Debug)]
pub enum BoundaryError {
    /// c > N: the skid margin failed on this host. Fatal, DATA_LOSS class
    /// — the segment cannot be trusted (risk R1).
    Overshoot {
        target: u64,
        counted: u64,
    },
    Counter(CounterError),
    Kvm(String),
    /// The exit handler declared a fatal condition.
    Exit(String),
}

impl std::fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoundaryError::Overshoot { target, counted } => write!(
                f,
                "OVERSHOOT: counted {counted} past target {target} (skid margin too small)"
            ),
            BoundaryError::Counter(e) => write!(f, "counter: {e:?}"),
            BoundaryError::Kvm(e) => write!(f, "kvm: {e}"),
            BoundaryError::Exit(e) => write!(f, "exit handler: {e}"),
        }
    }
}

/// Run the vCPU until the guest has retired exactly `target` instructions
/// (absolute, counter-relative — i.e. `counter.read()` space).
///
/// Preconditions (run-control wiring): the kick handler is installed
/// process-wide, the counter is enabled and routed to THIS thread, and
/// the vCPU is at an instruction boundary.
///
/// `on_exit` services every non-debug, non-kick exit (device MMIO/PIO,
/// IN-fill, …) — INCLUDING `Hlt` and `Shutdown`: whether a halt during a
/// landing is fatal is run control's call, not this engine's. Counting is
/// unaffected by servicing — an instruction that exited mid-emulation has
/// not retired.
///
/// Error/result precedence: if the boundary lands but DROPPING single-step
/// fails, the caller gets that error, not the boundary — a vCPU left in
/// single-step is R10-fatal and must never be resumed as healthy.
///
/// PERIOD semantics this engine relies on (and the live tests prove):
/// PERF_EVENT_IOC_PERIOD takes effect immediately from the current count
/// (Linux ≥ 3.14), so arm_period(d − SKID) fires after that many MORE
/// retirements.
pub fn land_at(
    vcpu: &mut VcpuFd,
    counter: &InstRetired,
    target: u64,
    margins: &Margins,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
) -> Result<Boundary, BoundaryError> {
    let mut guard = KickGuard::register(vcpu);
    let mut stepping = false;
    let result = loop {
        let c = counter.read().map_err(BoundaryError::Counter)?;
        if c > target {
            break Err(BoundaryError::Overshoot { target, counted: c });
        }
        if c == target {
            // Retirement is the only thing that moves the counter, so we
            // are at an instruction start (mid-REP iterations and
            // mid-emulation exits never increment it — module docs).
            let regs = guard
                .get_regs()
                .map_err(|e| BoundaryError::Kvm(format!("KVM_GET_REGS: {e}")))?;
            break Ok(Boundary {
                icount: c,
                rip: regs.rip,
                rcx: regs.rcx,
            });
        }
        let d = target - c;
        if !stepping && d > margins.skid_margin + margins.resync_slack {
            // Far approach: one arm per approach, then run.
            counter
                .arm_period(d - margins.skid_margin)
                .map_err(BoundaryError::Counter)?;
            match guard.run() {
                Ok(exit) => on_exit(exit)?,
                Err(e) if e.errno() == libc::EINTR => {
                    // The PMI kick (or a stale queued kick): a stop
                    // REQUEST, not a boundary assertion — loop re-reads.
                    clear_immediate_exit(&mut guard);
                }
                Err(e) => break Err(BoundaryError::Kvm(format!("KVM_RUN: {e}"))),
            }
        } else {
            if !stepping {
                // Near approach: park the PMI period (it STAYS enabled,
                // §3.2) so a tight re-arm loop cannot trip the sample-rate
                // throttle, then turn on single-step.
                counter
                    .arm_period(NEVER_FIRES_PERIOD)
                    .map_err(BoundaryError::Counter)?;
                set_singlestep(&mut guard, true)?;
                stepping = true;
            }
            match guard.run() {
                // One step. The counter re-read at loop top is the ONLY
                // progress signal (never assume +1; REP rule).
                Ok(VcpuExit::Debug(_)) => {}
                Ok(exit) => on_exit(exit)?,
                Err(e) if e.errno() == libc::EINTR => {
                    clear_immediate_exit(&mut guard);
                }
                Err(e) => break Err(BoundaryError::Kvm(format!("KVM_RUN: {e}"))),
            }
        }
    };
    if stepping {
        // Drop KVM_GUESTDBG_ENABLE the moment the boundary is reached —
        // also on the error paths, so no caller ever observes a vCPU left
        // in single-step (risk R10).
        set_singlestep(&mut guard, false)?;
    }
    result
}

fn set_singlestep(vcpu: &mut VcpuFd, on: bool) -> Result<(), BoundaryError> {
    let control = if on {
        kvm_bindings::KVM_GUESTDBG_ENABLE | kvm_bindings::KVM_GUESTDBG_SINGLESTEP
    } else {
        0
    };
    let dbg = kvm_guest_debug {
        control,
        ..Default::default()
    };
    vcpu.set_guest_debug(&dbg)
        .map_err(|e| BoundaryError::Kvm(format!("KVM_SET_GUEST_DEBUG: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::load_and_enter;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    /// Boot the landing-loop guest and return (slot, counter) ready for
    /// land_at: counter enabled, routed, kick handler installed.
    fn landing_rig() -> Option<(crate::kvm::SlotVm, InstRetired)> {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return None;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(16 << 20).unwrap();
        // Plenty of iterations: the tests land well before it finishes.
        load_and_enter(&slot, nanokernel::landing_loop_elf(), b"1000000000").unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();
        Some((slot, counter))
    }

    fn no_exits(exit: VcpuExit) -> Result<(), BoundaryError> {
        Err(BoundaryError::Exit(format!(
            "unexpected guest exit during landing: {exit:?}"
        )))
    }

    #[test]
    fn lands_exactly_via_pmi_then_step_live() {
        let Some((mut slot, counter)) = landing_rig() else {
            return;
        };
        // Far target: PMI approach plus the stepped tail.
        let b = land_at(
            &mut slot.vcpu,
            &counter,
            1_000_000,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap();
        assert_eq!(b.icount, 1_000_000);
        assert_eq!(counter.read().unwrap(), 1_000_000, "exactly N, not past");

        // Land again further out: the engine composes across calls.
        let b2 = land_at(
            &mut slot.vcpu,
            &counter,
            2_500_000,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap();
        assert_eq!(b2.icount, 2_500_000);
        assert!(b2.rip >= 0x10_0000, "rip inside the guest image");
    }

    #[test]
    fn lands_exactly_with_pure_single_step_live() {
        let Some((mut slot, counter)) = landing_rig() else {
            return;
        };
        // First get into the loop body a bit (far landing).
        land_at(
            &mut slot.vcpu,
            &counter,
            100_000,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap();
        // Near target: under skid+slack, never arms a real period.
        let b = land_at(
            &mut slot.vcpu,
            &counter,
            100_123,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap();
        assert_eq!(b.icount, 100_123);
    }

    #[test]
    fn landing_is_deterministic_across_boots_live() {
        let run = || {
            let (mut slot, counter) = landing_rig()?;
            let b = land_at(
                &mut slot.vcpu,
                &counter,
                777_777,
                &Margins::default(),
                &mut no_exits,
            )
            .unwrap();
            Some((b.icount, b.rip, b.rcx))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "same guest, same target, same boundary tuple");
    }

    #[test]
    fn stale_target_is_a_loud_overshoot_live() {
        let Some((mut slot, counter)) = landing_rig() else {
            return;
        };
        land_at(
            &mut slot.vcpu,
            &counter,
            50_000,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap();
        // A target already in the past must be Overshoot, never absorbed.
        let err = land_at(
            &mut slot.vcpu,
            &counter,
            10_000,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            BoundaryError::Overshoot {
                target: 10_000,
                counted: 50_000
            }
        ));
    }
}
