//! Run control (ARCH §3.3, Phase-1 slice of bead qs4): execute one Run
//! segment — compile the agenda, walk its stop points with the §3.2
//! boundary engine, inject scheduled vectors per §3.4, hash epoch
//! boundaries, poll goals, and honor asynchronous Pause by ROLLING
//! FORWARD to the next epoch boundary before reporting paused
//! (pause-soon-at-a-deterministic-point; latency ≤ epoch_len).
//!
//! Phase-1 scope: `Until::{IcountBudget, VnsBudget, Goal}` are live;
//! `NextSdkEvent` and `FrameBudget` need the device-bus run loop (M1
//! acceptance bead) and return [`RunError::NotYetWired`] — the enum shape
//! stays aligned with API.md §2.4 so M6's gRPC Run maps 1:1. Margins come
//! from MachineConfig (single source of truth — bead srz).

use std::sync::atomic::{AtomicBool, Ordering};

use dh_detclock::counter::InstRetired;
use kvm_ioctls::VcpuExit;

use crate::agenda::{compile, AgendaError, AgendaInputs, FinalStop, StopKind};
use crate::boundary::{land_at, Boundary, BoundaryError, Margins};
use crate::config::MachineConfig;
use crate::hash::StateHashChain;
use crate::inject::{inject_at_boundary, InjectError, Injection};
use crate::kvm::SlotVm;

/// API.md §2.4 `Run.until`, Phase-1 shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// Stop after this many MORE instructions.
    IcountBudget(u64),
    /// Stop at the first icount where vns advanced by at least this much.
    VnsBudget(u64),
    /// Poll a goal every `poll_period` instructions under a hard cap.
    Goal { poll_period: u64, hard_cap: u64 },
    /// Needs the device run loop (doorbell exits) — NotYetWired.
    NextSdkEvent,
    /// Needs the device run loop (FRAME_COUNTER exits) — NotYetWired.
    FrameBudget(u64),
}

/// Why the segment stopped (mirrors proto StopReason).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    BudgetReached,
    GoalSatisfied,
    HardCap,
    Paused,
}

/// A finished segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentOutcome {
    pub reason: StopReason,
    pub boundary: Boundary,
    pub vns: u64,
    pub state_hash: [u8; 32],
    /// Scheduled injections actually delivered (count; details logged by
    /// the caller per injection via the AUX record).
    pub injections_delivered: u64,
}

#[derive(Debug)]
pub enum RunError {
    Agenda(AgendaError),
    Boundary(BoundaryError),
    Inject(InjectError),
    Kvm(String),
    /// vns/icount conversion overflowed u64 space.
    ClockOverflow,
    /// The until-mode needs machinery a later bead wires (device loop).
    NotYetWired(&'static str),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Agenda(e) => write!(f, "agenda: {e:?}"),
            RunError::Boundary(e) => write!(f, "boundary: {e}"),
            RunError::Inject(e) => write!(f, "inject: {e}"),
            RunError::Kvm(e) => write!(f, "kvm: {e}"),
            RunError::ClockOverflow => write!(f, "vns/icount conversion overflow"),
            RunError::NotYetWired(what) => {
                write!(f, "{what} needs the device run loop (M1 acceptance bead)")
            }
        }
    }
}

/// One scheduled injection: vector at a segment-relative icount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledInjection {
    pub icount: u64,
    pub vector: u8,
}

/// Everything a Phase-1 segment needs. The counter is enabled and routed
/// to this thread; the vCPU sits at a boundary with `start_icount`
/// retirements already counted.
pub struct Segment<'a> {
    pub slot: &'a mut SlotVm,
    pub counter: &'a InstRetired,
    pub chain: &'a mut StateHashChain,
    pub config: &'a MachineConfig,
    pub start_icount: u64,
    pub injections: &'a [ScheduledInjection],
    /// Cooperative pause flag (another thread / signal handler sets it).
    pub pause: &'a AtomicBool,
}

/// Run one segment to its stop. `goal` is consulted only for
/// `Until::Goal` (at every poll boundary; returning true stops the run).
/// `on_exit` services device exits (Phase-1 guests: serial OUTs etc.).
pub fn run_segment(
    seg: &mut Segment<'_>,
    until: Until,
    goal: &mut dyn FnMut() -> bool,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
) -> Result<SegmentOutcome, RunError> {
    let clock = seg.config.clock;
    let margins = Margins {
        skid_margin: u64::from(seg.config.skid_margin),
        resync_slack: u64::from(seg.config.resync_slack),
    };

    let (final_stop, goal_poll_period) = match until {
        Until::IcountBudget(b) => (FinalStop::IcountBudget(b), None),
        Until::VnsBudget(b) => (FinalStop::VnsBudget(b), None),
        Until::Goal {
            poll_period,
            hard_cap,
        } => (
            FinalStop::HardCap(hard_cap),
            std::num::NonZeroU64::new(poll_period),
        ),
        Until::NextSdkEvent => return Err(RunError::NotYetWired("next_sdk_event")),
        Until::FrameBudget(_) => return Err(RunError::NotYetWired("frame_budget")),
    };

    let injection_icounts: Vec<u64> = seg.injections.iter().map(|i| i.icount).collect();
    let inputs = AgendaInputs {
        start_icount: seg.start_icount,
        injections: &injection_icounts,
        epoch_len: std::num::NonZeroU64::new(seg.config.epoch_len),
        goal_poll_period,
        final_stop,
        clock,
    };
    let agenda = compile(&inputs).map_err(RunError::Agenda)?;

    let mut delivered = 0u64;
    for point in &agenda {
        let boundary = land_at(
            &mut seg.slot.vcpu,
            seg.counter,
            point.icount,
            &margins,
            on_exit,
        )
        .map_err(RunError::Boundary)?;

        // Scheduled injections at this point (§3.4; one vector per entry —
        // inject_at_boundary steps between queued vectors when several
        // share a boundary, so each gets its own VM entry).
        let mut at = boundary;
        for idx in &point.injections {
            let inj: Injection = inject_at_boundary(
                &mut seg.slot.vcpu,
                seg.counter,
                seg.injections[*idx].vector,
                &at,
                &margins,
                seg.config.epoch_len,
                on_exit,
            )
            .map_err(RunError::Inject)?;
            delivered += 1;
            at = Boundary {
                icount: inj.delivered_icount,
                rip: inj.delivered_rip,
                rcx: at.rcx,
            };
        }

        let vns = clock
            .vns_from_icount(point.icount)
            .ok_or(RunError::ClockOverflow)?;

        if point.epoch_hash {
            let sections = Vec::new(); // device bus arrives with the M1 loop
            seg.chain
                .push_final_link(seg.slot, &sections, point.icount, vns)
                .map_err(|e| RunError::Kvm(format!("{e:?}")))?;
        }

        if point.goal_poll && goal() {
            return finish(seg, StopReason::GoalSatisfied, boundary, vns, delivered);
        }

        if let Some(kind) = point.final_stop {
            let reason = match kind {
                StopKind::Budget => StopReason::BudgetReached,
                StopKind::HardCap => StopReason::HardCap,
            };
            return finish(seg, reason, boundary, vns, delivered);
        }

        // Asynchronous Pause (§3.3): honored at deterministic points only —
        // roll FORWARD to the next epoch boundary, hash it, report Paused.
        if seg.pause.load(Ordering::Relaxed) {
            let epoch = seg.config.epoch_len.max(1);
            let next_epoch = point.icount.div_ceil(epoch).max(1) * epoch;
            let b = land_at(
                &mut seg.slot.vcpu,
                seg.counter,
                next_epoch,
                &margins,
                on_exit,
            )
            .map_err(RunError::Boundary)?;
            let vns = clock
                .vns_from_icount(b.icount)
                .ok_or(RunError::ClockOverflow)?;
            seg.chain
                .push_final_link(seg.slot, &[], b.icount, vns)
                .map_err(|e| RunError::Kvm(format!("{e:?}")))?;
            return Ok(SegmentOutcome {
                reason: StopReason::Paused,
                boundary: b,
                vns,
                state_hash: seg.chain.value(),
                injections_delivered: delivered,
            });
        }
    }
    unreachable!("agenda always carries exactly one final stop point");
}

fn finish(
    seg: &mut Segment<'_>,
    reason: StopReason,
    boundary: Boundary,
    vns: u64,
    delivered: u64,
) -> Result<SegmentOutcome, RunError> {
    // Every segment ends with a hash link at its stop boundary (§8.5:
    // "at every final pause").
    seg.chain
        .push_final_link(seg.slot, &[], boundary.icount, vns)
        .map_err(|e| RunError::Kvm(format!("{e:?}")))?;
    Ok(SegmentOutcome {
        reason,
        boundary,
        vns,
        state_hash: seg.chain.value(),
        injections_delivered: delivered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::load_and_enter;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;
    use dh_detclock::counter::NEVER_FIRES_PERIOD;

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    fn test_config() -> MachineConfig {
        MachineConfig::new(
            16 << 20,
            [0; 32],
            crate::config::BootSpec::Elf {
                kernel_hash: [0; 32],
                cmdline: Vec::new(),
            },
        )
    }

    fn rig() -> Option<(crate::kvm::SlotVm, InstRetired)> {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return None;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(16 << 20).unwrap();
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
        Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}")))
    }

    fn never() -> bool {
        false
    }

    #[test]
    fn icount_budget_runs_twice_identically_live() {
        let run = || {
            let (mut slot, counter) = rig()?;
            let config = test_config();
            let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
            let pause = AtomicBool::new(false);
            let mut seg = Segment {
                slot: &mut slot,
                counter: &counter,
                chain: &mut chain,
                config: &config,
                start_icount: 0,
                injections: &[],
                pause: &pause,
            };
            let out = run_segment(
                &mut seg,
                Until::IcountBudget(300_000),
                &mut never,
                &mut no_exits,
            )
            .unwrap();
            assert_eq!(out.reason, StopReason::BudgetReached);
            assert_eq!(out.boundary.icount, 300_000);
            Some((
                out.boundary.icount,
                out.boundary.rip,
                out.vns,
                out.state_hash,
            ))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "RUN-TWICE-COMPARE: full outcome must be identical");
    }

    #[test]
    fn goal_poll_stops_at_poll_boundary_live() {
        let Some((mut slot, counter)) = rig() else {
            return;
        };
        let config = test_config();
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            pause: &pause,
        };
        let mut polls = 0u32;
        let out = run_segment(
            &mut seg,
            Until::Goal {
                poll_period: 10_000,
                hard_cap: 1_000_000,
            },
            &mut || {
                polls += 1;
                polls == 3
            },
            &mut no_exits,
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::GoalSatisfied);
        assert_eq!(out.boundary.icount, 30_000, "third RUN-relative poll");
        assert_eq!(polls, 3);
    }

    #[test]
    fn pause_rolls_forward_to_the_epoch_boundary_live() {
        let Some((mut slot, counter)) = rig() else {
            return;
        };
        let config = test_config();
        let epoch = config.epoch_len;
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(true); // armed before the run
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            pause: &pause,
        };
        let out = run_segment(
            &mut seg,
            Until::IcountBudget(50 * epoch),
            &mut never,
            &mut no_exits,
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::Paused);
        assert!(out.boundary.icount.is_multiple_of(epoch) && out.boundary.icount > 0);
        assert!(
            out.boundary.icount <= 2 * epoch,
            "pause latency must be <= epoch_len from the first checked point"
        );
    }

    #[test]
    fn unwired_modes_fail_loudly() {
        let Some((mut slot, counter)) = rig() else {
            return;
        };
        let config = test_config();
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            pause: &pause,
        };
        assert!(matches!(
            run_segment(&mut seg, Until::NextSdkEvent, &mut never, &mut no_exits),
            Err(RunError::NotYetWired("next_sdk_event"))
        ));
        assert!(matches!(
            run_segment(&mut seg, Until::FrameBudget(3), &mut never, &mut no_exits),
            Err(RunError::NotYetWired("frame_budget"))
        ));
    }
}
