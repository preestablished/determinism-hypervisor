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
use std::sync::atomic::{AtomicU64, Ordering};

use crate::run::{clear_immediate_exit, KickGuard};

static LANDING_SINGLE_STEPS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Process-wide count of KVM entries made with guest single-step enabled
/// by the boundary/injection machinery. `dh-workerd` exports this as the
/// ARCH §9 landing single-step telemetry.
pub fn landing_single_steps_total() -> u64 {
    LANDING_SINGLE_STEPS_TOTAL.load(Ordering::Relaxed)
}

fn record_single_step_entry() {
    LANDING_SINGLE_STEPS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// §3.2 margins (MachineConfig fields; defaults are the doc's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Margins {
    pub skid_margin: u64,
    pub resync_slack: u64,
}

impl Default for Margins {
    fn default() -> Self {
        Margins {
            skid_margin: u64::from(crate::config::DEFAULT_SKID_MARGIN),
            resync_slack: u64::from(crate::config::DEFAULT_RESYNC_SLACK),
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
            record_single_step_entry();
            match guard.run() {
                // One step. The counter re-read at loop top is the ONLY
                // progress signal (never assume +1; REP rule).
                //
                // MEASURED (iteration 83, bead 4a3, kernel 6.8 on the
                // lab box — the exact kernel mechanism may differ on
                // other versions; the re-arm is correct regardless): a Debug
                // exit DELIVERED BY THE EMULATOR's completion path (the
                // singlestep hook that fires when an emulated MMIO
                // instruction finishes) CONSUMES the guest_debug arming —
                // without the re-assert below, the entry after such a
                // Debug exit free-runs to the next natural exit (the
                // mmio_stepper probe measured +18, the iteration-82
                // goal-poll overshoot +74). Hardware-delivered #DBs do
                // not need it, but the two are indistinguishable here
                // and the re-assert is idempotent — re-arm on EVERY
                // step.
                Ok(VcpuExit::Debug(_)) => {
                    set_singlestep(&mut guard, true)?;
                }
                Ok(exit) => {
                    on_exit(exit)?;
                    // MEASURED (iteration 50, kernel 6.8): an MMIO-WRITE
                    // exit eats the pending single-step trap — the
                    // emulator completes the instruction and clears TF
                    // without delivering the #DB, so re-entry would
                    // FREE-RUN past the target (measured: 991 instrs to
                    // the next exit; loud Overshoot). MMIO reads and PIO
                    // keep the trap. Re-asserting guest_debug re-arms
                    // TF; harmless where the trap survived.
                    set_singlestep(&mut guard, true)?;
                }
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

/// CROSS-REFERENCE (iteration 83, bead 4a3): land_at's stepping loop
/// re-arms guest_debug on every Debug exit because the EMULATOR's
/// completion-path Debug delivery consumes the arming. This helper has
/// a different structure — it returns at the FIRST Debug, so a consumed
/// arming cannot free-run within one call, and callers re-arm per
/// entry. Do not blindly mirror the land_at re-arm here; the open
/// question (injection chains stepping ACROSS an emulated MMIO
/// instruction, reachable once the M5 device run loop delivers
/// interrupts adjacent to MMIO) is tracked by its own bead.
///
/// One guest ENTRY under single-step: enter once, return at the next
/// debug trap. NOT one retirement — an entry that delivers a pending
/// interrupt runs the whole handler before the trap fires (event delivery
/// suppresses the step until the gate sequence + ISR complete), so the
/// returned boundary can be many retirements ahead. Deterministic; used
/// to chain same-boundary injections (one queued vector per entry).
///
/// The same suppression means §3.2 NEAR-approach landings must never
/// target an icount strictly inside a delivery window — the engine would
/// overshoot LOUDLY (the M6 scheduler owns avoiding such targets).
///
/// PRECONDITION on `on_exit`: it must treat Hlt (and anything else that
/// re-enters with fresh forward progress) as an ERROR — an Ok-handled
/// Hlt would re-run and silently chain MULTIPLE logical entries under
/// one "step" (run_segment's wrapper guarantees this; a future device
/// run loop caller must too).
pub fn step_one_entry(
    vcpu: &mut VcpuFd,
    counter: &InstRetired,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
) -> Result<Boundary, BoundaryError> {
    let mut guard = KickGuard::register(vcpu);
    set_singlestep(&mut guard, true)?;
    let result = loop {
        record_single_step_entry();
        match guard.run() {
            Ok(VcpuExit::Debug(_)) => break Ok(()),
            Ok(exit) => {
                if let Err(e) = on_exit(exit) {
                    break Err(e);
                }
                // Re-arm TF: an MMIO-write exit clears it without the
                // #DB (see land_at; measured iteration 50). NOTE the
                // entry then traps after the NEXT instruction — the
                // write already completed, so one entry can span the
                // write plus its successor.
                set_singlestep(&mut guard, true)?;
            }
            Err(e) if e.errno() == libc::EINTR => clear_immediate_exit(&mut guard),
            Err(e) => break Err(BoundaryError::Kvm(format!("KVM_RUN: {e}"))),
        }
    };
    set_singlestep(&mut guard, false)?;
    result?;
    let icount = counter.read().map_err(BoundaryError::Counter)?;
    // Forward progress is the one invariant the relaxed (non-targeted)
    // step keeps: at least one retirement happened.
    debug_assert!(icount > 0, "step_one_entry made no progress");
    let regs = guard
        .get_regs()
        .map_err(|e| BoundaryError::Kvm(format!("KVM_GET_REGS: {e}")))?;
    Ok(Boundary {
        icount,
        rip: regs.rip,
        rcx: regs.rcx,
    })
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
    use crate::inject::inject_at_boundary;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;
    use vm_memory::{Bytes, GuestAddress};

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
        let steps_before = landing_single_steps_total();
        let b = land_at(
            &mut slot.vcpu,
            &counter,
            100_123,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap();
        assert_eq!(b.icount, 100_123);
        assert!(
            landing_single_steps_total() > steps_before,
            "near landing must publish single-step telemetry"
        );
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

    /// PROBE-AUTHORING LESSON (iteration 83): raw code injected at
    /// rip=0 runs in REAL MODE — 64-bit encodings misdecode and the
    /// MMIO hole at 0xD000_1000 is unreachable, so such probes pass
    /// VACUOUSLY. MMIO-landing probes must boot a long-mode nanokernel
    /// guest (below).
    ///
    /// Iteration-83 probe (bead 4a3): land EXACTLY inside the
    /// mmio_stepper guest — a long-mode loop whose body is the doorbell
    /// cluster (imm dword MMIO write, 8-byte reg MMIO write, MMIO read,
    /// 16 nops). The single-step walk must cross hundreds of emulated
    /// MMIO exits without losing the trap; the iteration-82 goal-poll
    /// overshoot (target 4096, counted 4170) is THIS path. Targets are
    /// chosen deep in the loop so the walk spans many clusters; with
    /// production margins every landing here is pure stepping.
    fn stepper_rig() -> Option<(crate::kvm::SlotVm, InstRetired)> {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return None;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, nanokernel::mmio_stepper_elf(), b"").unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();
        Some((slot, counter))
    }

    fn ack_mmio(exit: VcpuExit) -> Result<(), BoundaryError> {
        match exit {
            VcpuExit::MmioWrite(_, _) => Ok(()),
            VcpuExit::MmioRead(_, data) => {
                data.fill(0);
                Ok(())
            }
            other => Err(BoundaryError::Exit(format!("unexpected exit {other:?}"))),
        }
    }

    fn irq_stepper_rig() -> Option<(crate::kvm::SlotVm, InstRetired)> {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return None;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, nanokernel::mmio_irq_stepper_elf(), b"").unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();
        Some((slot, counter))
    }

    fn read_irq_stepper_table(slot: &crate::kvm::SlotVm) -> (u64, Vec<u8>) {
        let mut head = [0u8; 8];
        slot.guest_mem
            .read_slice(
                &mut head,
                GuestAddress(nanokernel::MMIO_IRQ_STEPPER_TABLE_GPA),
            )
            .unwrap();
        let count = u64::from_le_bytes(head);
        let mut vecs = vec![0u8; count as usize];
        slot.guest_mem
            .read_slice(
                &mut vecs,
                GuestAddress(nanokernel::MMIO_IRQ_STEPPER_TABLE_GPA + 8),
            )
            .unwrap();
        (count, vecs)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum StepExit {
        MmioWrite(u64),
        MmioRead(u64),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct StepProfile {
        before: Boundary,
        after: Boundary,
        exits: Vec<StepExit>,
    }

    const IRQ_STEPPER_MMIO_BASE: u64 = 0xD000_1000;
    const IRQ_STEPPER_MMIO_CLUSTER: [StepExit; 3] = [
        StepExit::MmioWrite(IRQ_STEPPER_MMIO_BASE + 0x14),
        StepExit::MmioWrite(IRQ_STEPPER_MMIO_BASE + 0x08),
        StepExit::MmioRead(IRQ_STEPPER_MMIO_BASE + 0x18),
    ];

    fn current_boundary(slot: &crate::kvm::SlotVm, counter: &InstRetired) -> Boundary {
        let regs = slot.vcpu.get_regs().unwrap();
        Boundary {
            icount: counter.read().unwrap(),
            rip: regs.rip,
            rcx: regs.rcx,
        }
    }

    fn profile_step(slot: &mut crate::kvm::SlotVm, counter: &InstRetired) -> StepProfile {
        let before = current_boundary(slot, counter);
        let mut exits = Vec::new();
        let after = step_one_entry(&mut slot.vcpu, counter, &mut |exit| match exit {
            VcpuExit::MmioWrite(gpa, _) => {
                exits.push(StepExit::MmioWrite(gpa));
                Ok(())
            }
            VcpuExit::MmioRead(gpa, data) => {
                exits.push(StepExit::MmioRead(gpa));
                data.fill(0);
                Ok(())
            }
            other => Err(BoundaryError::Exit(format!("unexpected exit {other:?}"))),
        })
        .unwrap();
        StepProfile {
            before,
            after,
            exits,
        }
    }

    fn measure_isr_delta(pure: &StepProfile) -> Option<u64> {
        let (mut slot, counter) = irq_stepper_rig()?;
        let at = land_at(
            &mut slot.vcpu,
            &counter,
            pure.before.icount,
            &Margins::default(),
            &mut ack_mmio,
        )
        .unwrap();
        assert_eq!(at, pure.before, "pure-step measurement landed elsewhere");

        let inj = inject_at_boundary(
            &mut slot.vcpu,
            &counter,
            0x40,
            &at,
            &Margins::default(),
            16,
            &mut ack_mmio,
        )
        .unwrap();
        assert_eq!(inj.delivered_icount, pure.before.icount);
        assert_eq!(inj.delivered_rip, pure.before.rip);

        let after = profile_step(&mut slot, &counter);
        assert!(after.exits.is_empty(), "ISR delta step should stay in NOPs");
        assert_eq!(after.before, pure.before);
        assert_eq!(after.after.rip, pure.after.rip);
        assert_eq!(after.after.rcx, pure.after.rcx);
        assert!(
            after.after.icount > pure.after.icount,
            "interrupt delivery must retire the ISR before the guest step"
        );
        let (_, vecs) = read_irq_stepper_table(&slot);
        assert_eq!(vecs, vec![0x40]);
        Some(after.after.icount - pure.after.icount)
    }

    fn discover_mmio_profiles() -> Option<(StepProfile, StepProfile, u64)> {
        let (mut slot, counter) = irq_stepper_rig()?;
        let mut pre_mmio_pure = None;
        for _ in 0..512 {
            let mmio = profile_step(&mut slot, &counter);
            if mmio.exits.as_slice() == IRQ_STEPPER_MMIO_CLUSTER.as_slice() {
                let pre_mmio_pure =
                    pre_mmio_pure.expect("mmio_irq_stepper should have pure setup steps");
                let post_mmio_pure = profile_step(&mut slot, &counter);
                assert_eq!(
                    post_mmio_pure.before, mmio.after,
                    "the post-MMIO boundary should be the next step's start"
                );
                assert!(
                    post_mmio_pure.exits.is_empty(),
                    "post-MMIO step should be a NOP"
                );
                assert_eq!(
                    post_mmio_pure.after.icount,
                    post_mmio_pure.before.icount + 1,
                    "post-MMIO pure step should retire one instruction"
                );
                let isr_delta = measure_isr_delta(&pre_mmio_pure)?;
                return Some((mmio, post_mmio_pure, isr_delta));
            }
            if mmio.exits.is_empty() {
                pre_mmio_pure = Some(mmio);
            }
        }
        panic!("mmio_irq_stepper did not expose an MMIO-crossing entry");
    }

    #[test]
    fn step_one_entry_chained_injection_crosses_mmio_exactly_live() {
        let Some((mmio, pure, isr_delta)) = discover_mmio_profiles() else {
            return;
        };
        assert!(isr_delta > 0);

        let run = || {
            let (mut slot, counter) = irq_stepper_rig()?;
            let at = land_at(
                &mut slot.vcpu,
                &counter,
                mmio.before.icount,
                &Margins::default(),
                &mut ack_mmio,
            )
            .unwrap();
            assert_eq!(at, mmio.before, "fresh run must land at the same RIP");

            let inj1 = inject_at_boundary(
                &mut slot.vcpu,
                &counter,
                0x40,
                &at,
                &Margins::default(),
                16,
                &mut ack_mmio,
            )
            .unwrap();
            assert_eq!(
                inj1.delivered_icount, mmio.before.icount,
                "open window queues at the requested MMIO-adjacent boundary"
            );
            assert_eq!(inj1.delivered_rip, mmio.before.rip);

            let after_first = profile_step(&mut slot, &counter);
            assert_eq!(after_first.before, mmio.before);
            assert_eq!(after_first.exits, mmio.exits);
            assert_eq!(after_first.after.rip, mmio.after.rip);
            assert_eq!(after_first.after.rcx, mmio.after.rcx);
            assert_eq!(
                after_first.after.icount,
                mmio.after.icount + isr_delta,
                "MMIO-crossing entry must stop at the exact baseline boundary plus ISR retirements"
            );

            let inj2 = inject_at_boundary(
                &mut slot.vcpu,
                &counter,
                0x41,
                &after_first.after,
                &Margins::default(),
                16,
                &mut ack_mmio,
            )
            .unwrap();
            assert_eq!(
                inj2.delivered_icount, after_first.after.icount,
                "second same-boundary vector queues at step_one_entry's boundary"
            );
            assert_eq!(inj2.delivered_rip, pure.before.rip);

            let after_second = profile_step(&mut slot, &counter);
            assert_eq!(after_second.before, after_first.after);
            assert!(after_second.exits.is_empty());
            assert_eq!(after_second.after.rip, pure.after.rip);
            assert_eq!(after_second.after.rcx, pure.after.rcx);
            assert_eq!(
                after_second.after.icount,
                after_first.after.icount + (pure.after.icount - pure.before.icount) + isr_delta,
                "second chained entry must stop after exactly one guest instruction plus ISR"
            );
            let (count, vecs) = read_irq_stepper_table(&slot);
            assert_eq!(count, 2);
            assert_eq!(vecs, vec![0x40, 0x41]);

            Some((at, after_first, after_second, vecs))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "MMIO-adjacent chained delivery must replay exactly");
    }

    /// The iteration-82 repro shape: one landing at 4096, mid-loop.
    #[test]
    fn landing_at_4096_across_mmio_clusters_is_exact_live() {
        let Some((mut slot, counter)) = stepper_rig() else {
            return;
        };
        let b = land_at(
            &mut slot.vcpu,
            &counter,
            4096,
            &Margins::default(),
            &mut ack_mmio,
        )
        .expect("landing across MMIO clusters");
        assert_eq!(b.icount, 4096);
    }

    /// Consecutive landings marching THROUGH the MMIO-dense region —
    /// strides 1..=23 against the 18-retirement loop body, so walks of
    /// every short length start from a spread of body offsets (not a
    /// full offset×stride product, but far beyond the single shape that
    /// reproduced the bug).
    #[test]
    fn consecutive_landings_across_mmio_clusters_are_exact_live() {
        let Some((mut slot, counter)) = stepper_rig() else {
            return;
        };
        let mut target = 100;
        for k in 0..120 {
            target += 1 + (k % 23);
            let b = land_at(
                &mut slot.vcpu,
                &counter,
                target,
                &Margins::default(),
                &mut ack_mmio,
            )
            .unwrap_or_else(|e| panic!("landing {k} at {target}: {e:?}"));
            assert_eq!(b.icount, target, "landing {k}");
        }
    }
}
