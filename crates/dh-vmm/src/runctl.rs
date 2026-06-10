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
    /// The guest executed a terminal HLT mid-segment (proto GUEST_HALTED).
    GuestHalted,
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

    // The agenda is computed in counter space: a caller-asserted
    // start_icount that disagrees with the counter lands every point
    // wrong. Loud, early.
    let actual = seg
        .counter
        .read()
        .map_err(|e| RunError::Kvm(format!("counter: {e:?}")))?;
    if actual != seg.start_icount {
        return Err(RunError::Kvm(format!(
            "start_icount {} != counter {actual}",
            seg.start_icount
        )));
    }

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
        // FinalOnly drops the epoch HASH grid; the pause roll-forward grid
        // below is independent config arithmetic either way.
        epoch_len: match seg.config.hash_epochs {
            crate::config::HashEpochs::EpochsOn => std::num::NonZeroU64::new(seg.config.epoch_len),
            crate::config::HashEpochs::FinalOnly => None,
        },
        goal_poll_period,
        final_stop,
        clock,
    };
    let agenda = compile(&inputs).map_err(RunError::Agenda)?;

    // Terminal HLT (proto GUEST_HALTED) is a STOP, not a fault: the
    // wrapper flags it and unwinds the landing loop via a sentinel error.
    let mut halted = false;
    macro_rules! exits {
        () => {
            &mut |exit: VcpuExit| {
                if matches!(exit, VcpuExit::Hlt) {
                    halted = true;
                    return Err(BoundaryError::Exit("guest halted".into()));
                }
                on_exit(exit)
            }
        };
    }

    let mut delivered = 0u64;
    for point in &agenda {
        let landed = land_at(
            &mut seg.slot.vcpu,
            seg.counter,
            point.icount,
            &margins,
            exits!(),
        );
        let boundary = match landed {
            Ok(b) => b,
            Err(_) if halted => return finish_halted(seg, clock, delivered),
            Err(e) => return Err(RunError::Boundary(e)),
        };

        // Scheduled injections at this point (§3.4). KVM holds ONE queued
        // vector, and a second KVM_INTERRUPT before the next entry
        // silently OVERWRITES it (review, live-proven) — so between
        // vectors sharing a boundary the guest is entered for exactly one
        // retirement, delivering the queued vector before the next is
        // queued ("chained across consecutive entries", agenda docs).
        let mut at = boundary;
        for (i, idx) in point.injections.iter().enumerate() {
            if i > 0 {
                let stepped = land_at(
                    &mut seg.slot.vcpu,
                    seg.counter,
                    at.icount + 1,
                    &margins,
                    exits!(),
                );
                at = match stepped {
                    Ok(b) => b,
                    Err(_) if halted => return finish_halted(seg, clock, delivered),
                    Err(e) => return Err(RunError::Boundary(e)),
                };
            }
            let inj: Injection = inject_at_boundary(
                &mut seg.slot.vcpu,
                seg.counter,
                seg.injections[*idx].vector,
                &at,
                &margins,
                seg.config.epoch_len,
                exits!(),
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
            seg.chain
                .push_final_link(seg.slot, &[], point.icount, vns)
                .map_err(|e| RunError::Kvm(format!("{e:?}")))?;
        }

        // goal() must be a deterministic function of guest state (M6 goal
        // conditions read guest regions); a wall-clock-dependent closure
        // breaks replay identity — caller's burden, stated here.
        if point.goal_poll && goal() {
            return finish(
                seg,
                StopReason::GoalSatisfied,
                boundary,
                vns,
                delivered,
                point.epoch_hash,
            );
        }

        if let Some(kind) = point.final_stop {
            let reason = match kind {
                StopKind::Budget => StopReason::BudgetReached,
                StopKind::HardCap => StopReason::HardCap,
            };
            return finish(seg, reason, boundary, vns, delivered, point.epoch_hash);
        }

        // Asynchronous Pause (§3.3): honored at deterministic points only —
        // roll FORWARD to the next epoch boundary, hash it, report Paused.
        if seg.pause.load(Ordering::Relaxed) {
            let epoch = seg.config.epoch_len.max(1);
            let next_epoch = point.icount.div_ceil(epoch).max(1) * epoch;
            let rolled = land_at(
                &mut seg.slot.vcpu,
                seg.counter,
                next_epoch,
                &margins,
                exits!(),
            );
            let b = match rolled {
                Ok(b) => b,
                Err(_) if halted => return finish_halted(seg, clock, delivered),
                Err(e) => return Err(RunError::Boundary(e)),
            };
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
    already_hashed: bool,
) -> Result<SegmentOutcome, RunError> {
    // Every segment ends with a hash link at its stop boundary (§8.5: "at
    // every final pause") — exactly ONE link per boundary: a stop point
    // that is also an epoch-hash point was linked in the walk already.
    if !already_hashed {
        seg.chain
            .push_final_link(seg.slot, &[], boundary.icount, vns)
            .map_err(|e| RunError::Kvm(format!("{e:?}")))?;
    }
    Ok(SegmentOutcome {
        reason,
        boundary,
        vns,
        state_hash: seg.chain.value(),
        injections_delivered: delivered,
    })
}

/// Terminal HLT: stop where the guest stopped (proto GUEST_HALTED).
fn finish_halted(
    seg: &mut Segment<'_>,
    clock: crate::vt::ClockRatio,
    delivered: u64,
) -> Result<SegmentOutcome, RunError> {
    let icount = seg
        .counter
        .read()
        .map_err(|e| RunError::Kvm(format!("counter: {e:?}")))?;
    let regs = seg
        .slot
        .vcpu
        .get_regs()
        .map_err(|e| RunError::Kvm(format!("KVM_GET_REGS: {e}")))?;
    let boundary = Boundary {
        icount,
        rip: regs.rip,
        rcx: regs.rcx,
    };
    let vns = clock
        .vns_from_icount(icount)
        .ok_or(RunError::ClockOverflow)?;
    finish(
        seg,
        StopReason::GuestHalted,
        boundary,
        vns,
        delivered,
        false,
    )
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

#[cfg(test)]
mod halt_tests {
    use super::*;
    use crate::boot::load_and_enter;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;
    use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    /// pipeline_smoke parks in HLT a few dozen instructions in — a budget
    /// past the park must stop GUEST_HALTED with the serial output intact,
    /// never a fatal error (review: the proto defines GUEST_HALTED).
    #[test]
    fn terminal_hlt_is_a_stop_not_a_fault_live() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, nanokernel::pipeline_smoke_elf(), b"").unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();

        let config = {
            MachineConfig::new(
                16 << 20,
                [0; 32],
                crate::config::BootSpec::Elf {
                    kernel_hash: [0; 32],
                    cmdline: Vec::new(),
                },
            )
        };
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut serial = Vec::new();
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
            Until::IcountBudget(1_000_000),
            &mut || false,
            &mut |exit| {
                if let VcpuExit::IoOut(port, data) = exit {
                    if (0x3F8..0x400).contains(&port) {
                        serial.extend_from_slice(data);
                        return Ok(());
                    }
                }
                Err(BoundaryError::Exit(format!("unexpected: {exit:?}")))
            },
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::GuestHalted);
        assert_eq!(serial, b"K", "serial captured before the halt survives");
        assert!(out.boundary.icount < 1_000_000);
    }
}
