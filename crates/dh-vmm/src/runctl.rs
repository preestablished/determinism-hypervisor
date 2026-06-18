//! Run control (ARCH §3.3, Phase-1 slice of bead qs4): execute one Run
//! segment — compile the agenda, walk its stop points with the §3.2
//! boundary engine, inject scheduled vectors per §3.4, hash epoch
//! boundaries, poll goals, and honor asynchronous Pause by ROLLING
//! FORWARD to the next epoch boundary before reporting paused
//! (pause-soon-at-a-deterministic-point; latency ≤ epoch_len).
//!
//! All five API.md §2.4 until-modes are live (bead 4qo wired the two
//! event-driven ones). `FrameBudget` stops ON the Nth frame-boundary
//! exit (the pv-pad FRAME_COUNTER MMIO write, ARCH §6.6); `NextSdkEvent`
//! stops ON the doorbell exit whose drain produced a matching event —
//! matching is the CALLER's (the device rail applies the stream filter
//! and reports through [`Segment::sdk_events`]). Both exits are
//! guest-initiated at deterministic icounts, so the stop boundary
//! replays identically; both run under the request's hard cap. Margins
//! come from MachineConfig (single source of truth — bead srz).

use std::sync::atomic::{AtomicBool, Ordering};

use dh_detclock::counter::InstRetired;
use kvm_ioctls::VcpuExit;

use crate::agenda::{AgendaError, AgendaInputs, FinalStop, StopKind, compile};
use crate::boundary::{Boundary, BoundaryError, Margins, land_at};
use crate::config::MachineConfig;
use crate::hash::StateHashChain;
use crate::inject::{InjectError, Injection, inject_at_boundary};
use crate::kvm::SlotVm;

/// §3.4 deferral budget per injection: deterministic and loud when the
/// window never opens, but BOUNDED — each deferral step is a single-step
/// VM entry, so an epoch-sized budget (50M) would stall for minutes.
const INJECT_DEFER_BUDGET: u64 = 1 << 16;

/// API.md §2.4 `Run.until`, Phase-1 shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// Stop after this many MORE instructions.
    IcountBudget(u64),
    /// Stop at the first icount where vns advanced by at least this much.
    VnsBudget(u64),
    /// Poll a goal every `poll_period` instructions under a hard cap.
    Goal { poll_period: u64, hard_cap: u64 },
    /// Stop ON the doorbell exit whose drain produced a matching SDK
    /// event (API.md §2.4 `next_sdk_event`). The caller's exit servicing
    /// applies the stream filter and bumps [`Segment::sdk_events`];
    /// `hard_cap` is the request's safety net (proto `hard_icount_cap`).
    NextSdkEvent { hard_cap: u64 },
    /// Stop ON the frame-boundary exit (pv-pad FRAME_COUNTER MMIO write)
    /// of the Nth FrameMark since run start — the platform's only
    /// frame-quantized stop (API.md §2.4). Reports `BudgetReached`.
    FrameBudget { frames: u64, hard_cap: u64 },
}
// `hard_cap` is taken LITERALLY in both event-driven modes: the proto's
// "0 ⇒ worker default (10e9)" rule is the worker's RunRequest → Until
// mapping job, never runctl's — wiring a raw proto 0 through here caps
// the run at the start boundary.

/// Per-run audit knobs that do not change the machine identity. They are
/// caller policy, not [`MachineConfig`] preimage: replay/soak drivers must
/// pass the same options when comparing audit chains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunOptions {
    /// Force full-memory epoch links even when `MachineConfig.hash_epochs`
    /// is `FinalOnly`. This is deliberately expensive: it is a soak-run
    /// audit mode for dirty-tracking misses (risk R8).
    pub paranoid_hash: bool,
    /// Push the segment-final hash link when the requested `until` stop is
    /// reached at a non-epoch boundary. Normal recording runs do this; replay
    /// disables it for intermediate canonical-record landings because those
    /// are quantization points, not recorded segment stops.
    pub hash_final_stop: bool,
    /// Push an epoch hash when the requested `until` stop is itself an epoch
    /// boundary. Replay disables this for canonical-record landings so it can
    /// apply all records at that icount before verifying/logging the epoch.
    pub hash_final_epoch: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            paranoid_hash: false,
            hash_final_stop: true,
            hash_final_epoch: true,
        }
    }
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
    /// A matching SDK event stopped a `next_sdk_event` run (proto
    /// NEXT_SDK_EVENT); the caller returns the drained event itself.
    NextSdkEvent,
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
    /// The timer delivery, if the armed timer fired in this segment — the
    /// caller logs AUX TIMER_FIRE and disarms the device (one-shot).
    pub timer_fired: Option<TimerFired>,
    /// Count of frame-boundary exits (pv-pad FRAME_COUNTER MMIO writes)
    /// observed during this segment — maintained in EVERY until-mode and
    /// surfaced as RunResponse `frames_elapsed`. In the FrameBudget mode
    /// specifically it equals `frames` when the run stops BudgetReached.
    pub frames_elapsed: u64,
}

#[derive(Debug)]
pub enum RunError {
    Agenda(AgendaError),
    Boundary(BoundaryError),
    Inject(InjectError),
    Kvm(String),
    /// vns/icount conversion overflowed u64 space.
    ClockOverflow,
    /// `Until::NextSdkEvent` without [`Segment::sdk_events`]: the mode is
    /// caller-fed (the rail counts matching drained events) — running it
    /// without the feed would spin to the hard cap and report the wrong
    /// reason. Loud instead.
    MissingSdkEventFeed,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Agenda(e) => write!(f, "agenda: {e:?}"),
            RunError::Boundary(e) => write!(f, "boundary: {e}"),
            RunError::Inject(e) => write!(f, "inject: {e}"),
            RunError::Kvm(e) => write!(f, "kvm: {e}"),
            RunError::ClockOverflow => write!(f, "vns/icount conversion overflow"),
            RunError::MissingSdkEventFeed => {
                write!(
                    f,
                    "Until::NextSdkEvent requires Segment::sdk_events (the \
                     caller's matching-event count)"
                )
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

/// One frame-scheduled canonical input: apply `index` when the guest writes
/// absolute FRAME_COUNTER value `frame`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledFrameInput {
    pub frame: u32,
    pub index: usize,
}

/// A guest-armed one-shot pv-clock timer (ARCH §4): the caller reads it
/// from the device (`PvClock::armed()`) before compiling the segment.
/// `deadline_vns` is COUNTER-SPACE vns — the same origin as
/// `counter.read()` (zero at the counter reset, i.e. boot). Today that
/// origin never moves; when M4 restore gives segments a nonzero vns
/// base, the caller subtracts the base from the device's absolute
/// deadline BEFORE constructing this (review iter-41: the conversion
/// itself is origin-0; only the start clamp is segment-aware).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerArm {
    pub deadline_vns: u64,
    pub vector: u8,
}

/// ARCH §4 ceil rule: the timer's agenda icount is the smallest icount
/// whose vns reaches the deadline. A deadline at or before the segment
/// start clamps to the first boundary after start (the §3.4 deferral
/// applies either way; clock.rs documents the clamp as the caller's).
pub fn timer_to_injection(
    timer: TimerArm,
    clock: crate::vt::ClockRatio,
    start_icount: u64,
) -> Result<ScheduledInjection, RunError> {
    let icount = clock
        .icount_for_vns_target(timer.deadline_vns)
        .ok_or(RunError::ClockOverflow)?
        .max(start_icount + 1);
    Ok(ScheduledInjection {
        icount,
        vector: timer.vector,
    })
}

/// What a fired timer delivery looked like — the caller's AUX TIMER_FIRE
/// record (vector, armed_deadline_vns, delivered_icount) and its cue to
/// disarm the device (one-shot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerFired {
    pub vector: u8,
    pub armed_deadline_vns: u64,
    pub delivered_icount: u64,
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
    /// Guest-armed one-shot timer, if any (read from PvClock::armed()).
    pub timer: Option<TimerArm>,
    /// Cooperative pause flag (another thread / signal handler sets it).
    pub pause: &'a AtomicBool,
    /// `Until::NextSdkEvent`'s feed: a monotone count of MATCHING SDK
    /// events the caller's exit servicing has drained (the rail applies
    /// the request's stream filter and bumps this inside `on_exit`).
    /// runctl stops when it rises above its segment-start value. `None`
    /// is fine for every other mode; NextSdkEvent without it errs loud.
    pub sdk_events: Option<&'a std::cell::Cell<u64>>,
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
    run_segment_with_options(seg, until, RunOptions::default(), goal, on_exit)
}

/// [`run_segment`] with caller-supplied audit options and no epoch sink.
pub fn run_segment_with_options(
    seg: &mut Segment<'_>,
    until: Until,
    options: RunOptions,
    goal: &mut dyn FnMut() -> bool,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
) -> Result<SegmentOutcome, RunError> {
    run_segment_with_epoch_options(seg, until, options, goal, on_exit, &mut |_, _, _, _| Ok(()))
}

/// `run_segment` plus the M5 epoch-link sink (bead y62): every §8.5
/// chain link pushed AT AN EPOCH-GRID POINT during this quantum is
/// appended as `(epoch_index, icount, chain_value_after_push)` — the
/// caller (the recording rail) lands them as AUX EPOCH_HASH records.
/// The sink fires AT THE LINK POINT — between exits, in icount order —
/// because the DHILOG watermark is monotone: batching links after the
/// quantum would interleave them BEHIND later exit records and the
/// writer rejects the regression (measured in iteration 88). on_exit
/// and the sink typically share the recording rail through a RefCell
/// (single-threaded, never concurrently called). Final-pause links at
/// NON-epoch boundaries are NOT epoch hashes — they travel in
/// END.end_state_hash.
///
/// GRID ANCHORING (a5e/39w load-bearing): the epoch grid is ABSOLUTE in
/// counter space — multiples of `epoch_len` from counter zero, never
/// from a quantum's `start_icount` (agenda.rs compiles the grid that
/// way) — so record and replay runs quantized DIFFERENTLY still produce
/// the identical EPOCH_HASH set.
pub fn run_segment_with_epochs(
    seg: &mut Segment<'_>,
    until: Until,
    goal: &mut dyn FnMut() -> bool,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
    epoch_sink: &mut dyn FnMut(
        u64,
        Boundary,
        [u8; 32],
        Option<&SlotVm>,
    ) -> Result<(), BoundaryError>,
) -> Result<SegmentOutcome, RunError> {
    run_segment_with_epoch_options(seg, until, RunOptions::default(), goal, on_exit, epoch_sink)
}

/// [`run_segment`] plus scheduled canonical input landing points. At each
/// scheduled input boundary, `input_sink(index, boundary)` must apply the
/// caller-owned payload to device state/logging and return any edge IRQ
/// vectors the device queued for deterministic delivery at that boundary.
pub fn run_segment_with_scheduled_inputs(
    seg: &mut Segment<'_>,
    until: Until,
    scheduled_inputs: &[u64],
    goal: &mut dyn FnMut() -> bool,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
    input_sink: &mut dyn FnMut(usize, Boundary) -> Result<Vec<u8>, BoundaryError>,
) -> Result<SegmentOutcome, RunError> {
    run_segment_with_scheduled_inputs_and_frames(
        seg,
        until,
        scheduled_inputs,
        &[],
        0,
        goal,
        on_exit,
        input_sink,
    )
}

/// [`run_segment_with_scheduled_inputs`] plus dynamic frame-triggered
/// inputs. Frame inputs land immediately after the frame-boundary MMIO exit
/// is serviced, using the just-recorded absolute FRAME_COUNTER value.
pub fn run_segment_with_scheduled_inputs_and_frames(
    seg: &mut Segment<'_>,
    until: Until,
    scheduled_inputs: &[u64],
    frame_inputs: &[ScheduledFrameInput],
    start_frame_counter: u32,
    goal: &mut dyn FnMut() -> bool,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
    input_sink: &mut dyn FnMut(usize, Boundary) -> Result<Vec<u8>, BoundaryError>,
) -> Result<SegmentOutcome, RunError> {
    run_segment_with_scheduled_inputs_frames_and_epochs(
        seg,
        until,
        scheduled_inputs,
        frame_inputs,
        start_frame_counter,
        goal,
        on_exit,
        input_sink,
        &mut |_, _, _, _| Ok(()),
    )
}

/// [`run_segment_with_scheduled_inputs_and_frames`] plus observable epoch
/// links. Recorders use this to persist EPOCH_HASH records while preserving
/// the scheduled input landing contract. The final sink argument is
/// `Some(slot)` only when the epoch boundary has no transient queued vector
/// state and is therefore safe for restoreable checkpoint capture.
pub fn run_segment_with_scheduled_inputs_frames_and_epochs(
    seg: &mut Segment<'_>,
    until: Until,
    scheduled_inputs: &[u64],
    frame_inputs: &[ScheduledFrameInput],
    start_frame_counter: u32,
    goal: &mut dyn FnMut() -> bool,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
    input_sink: &mut dyn FnMut(usize, Boundary) -> Result<Vec<u8>, BoundaryError>,
    epoch_sink: &mut dyn FnMut(
        u64,
        Boundary,
        [u8; 32],
        Option<&SlotVm>,
    ) -> Result<(), BoundaryError>,
) -> Result<SegmentOutcome, RunError> {
    run_segment_inner(
        seg,
        until,
        RunOptions::default(),
        scheduled_inputs,
        frame_inputs,
        start_frame_counter,
        goal,
        on_exit,
        input_sink,
        epoch_sink,
    )
}

/// [`run_segment_with_epochs`] plus non-canonical run audit options.
pub fn run_segment_with_epoch_options(
    seg: &mut Segment<'_>,
    until: Until,
    options: RunOptions,
    goal: &mut dyn FnMut() -> bool,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
    epoch_sink: &mut dyn FnMut(
        u64,
        Boundary,
        [u8; 32],
        Option<&SlotVm>,
    ) -> Result<(), BoundaryError>,
) -> Result<SegmentOutcome, RunError> {
    run_segment_inner(
        seg,
        until,
        options,
        &[],
        &[],
        0,
        goal,
        on_exit,
        &mut |_, _| Ok(Vec::new()),
        epoch_sink,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_segment_inner(
    seg: &mut Segment<'_>,
    until: Until,
    options: RunOptions,
    scheduled_inputs: &[u64],
    frame_inputs: &[ScheduledFrameInput],
    start_frame_counter: u32,
    goal: &mut dyn FnMut() -> bool,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
    input_sink: &mut dyn FnMut(usize, Boundary) -> Result<Vec<u8>, BoundaryError>,
    epoch_sink: &mut dyn FnMut(
        u64,
        Boundary,
        [u8; 32],
        Option<&SlotVm>,
    ) -> Result<(), BoundaryError>,
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
        // The event-driven modes walk toward the request's hard cap; the
        // triggering exit unwinds the flight early (see exits! below).
        Until::NextSdkEvent { hard_cap } => (FinalStop::HardCap(hard_cap), None),
        Until::FrameBudget { hard_cap, .. } => (FinalStop::HardCap(hard_cap), None),
    };

    // Event-stop bookkeeping. Frame marks are decoded HERE (pure exit
    // decode against the device-side constants — pv-pad's FRAME_COUNTER
    // MMIO write); SDK-event matching is the CALLER's, consumed through
    // the Segment::sdk_events feed.
    let frame_mark_gpa = dh_devices::pad::PV_PAD_BASE + dh_devices::pad::REG_FRAME_COUNTER;
    let frame_target = match until {
        Until::FrameBudget { frames, .. } => Some(frames),
        _ => None,
    };
    let sdk_feed = match until {
        Until::NextSdkEvent { .. } => {
            let cell = seg.sdk_events.ok_or(RunError::MissingSdkEventFeed)?;
            // Baseline at segment start: the feed may be cumulative
            // across segments; only a rise during THIS one stops it.
            Some((cell, cell.get()))
        }
        _ => None,
    };
    let event_reason = match until {
        Until::FrameBudget { .. } => StopReason::BudgetReached,
        _ => StopReason::NextSdkEvent,
    };

    // FrameBudget(0): zero MORE frames — satisfied at the start boundary
    // without entering the guest (mirrors IcountBudget(0) semantics).
    if frame_target == Some(0) {
        return finish_at_counter(seg, clock, StopReason::BudgetReached, 0, None, 0);
    }

    // Merge the guest-armed timer (converted per the §4 ceil rule) into
    // the scheduled-injection set; remember which slot is the timer so
    // its delivery is reported for the AUX record.
    let mut all_injections: Vec<ScheduledInjection> = seg.injections.to_vec();
    let timer_slot = match seg.timer {
        Some(t) => {
            all_injections.push(timer_to_injection(t, clock, seg.start_icount)?);
            Some(all_injections.len() - 1)
        }
        None => None,
    };
    let injection_icounts: Vec<u64> = all_injections.iter().map(|i| i.icount).collect();
    let epoch_hashes_enabled =
        seg.config.hash_epochs == crate::config::HashEpochs::EpochsOn || options.paranoid_hash;
    let inputs = AgendaInputs {
        start_icount: seg.start_icount,
        scheduled_inputs,
        injections: &injection_icounts,
        // FinalOnly drops the epoch HASH grid unless paranoid audit mode
        // forces it back on; the pause roll-forward grid below is
        // independent config arithmetic either way.
        epoch_len: if epoch_hashes_enabled {
            std::num::NonZeroU64::new(seg.config.epoch_len)
        } else {
            None
        },
        goal_poll_period,
        final_stop,
        clock,
    };
    let agenda = compile(&inputs).map_err(RunError::Agenda)?;

    // Terminal HLT (proto GUEST_HALTED) is a STOP, not a fault: the
    // wrapper flags it and unwinds the landing loop via a sentinel error.
    // The event-driven stops use the same unwind: the triggering exit is
    // serviced by on_exit FIRST (the rail logs FRAME_MARK / drains the
    // doorbell at the exit's icount), THEN flagged — so the stop boundary
    // is the exit boundary, with the device state already current.
    let mut halted = false;
    let mut event_stop = false;
    let mut frames_seen = 0u64;
    let mut last_frame_counter = start_frame_counter;
    let mut frame_inputs_applied = vec![false; frame_inputs.len()];
    macro_rules! exits {
        () => {
            &mut |exit: VcpuExit| {
                if matches!(exit, VcpuExit::Hlt) {
                    halted = true;
                    return Err(BoundaryError::Exit("guest halted".into()));
                }
                let frame_mark = match &exit {
                    VcpuExit::MmioWrite(gpa, data)
                        if *gpa == frame_mark_gpa && data.len() == 4 =>
                    {
                        Some(u32::from_le_bytes((*data).try_into().unwrap()))
                    }
                    VcpuExit::MmioWrite(gpa, _) if *gpa == frame_mark_gpa => {
                        return Err(BoundaryError::Exit(
                            "FRAME_COUNTER write with non-u32 payload".into(),
                        ));
                    }
                    _ => None,
                };
                on_exit(exit)?;
                if let Some(frame) = frame_mark {
                    if frame <= last_frame_counter {
                        return Err(BoundaryError::Exit(format!(
                            "FRAME_COUNTER must increase monotonically: previous {last_frame_counter}, got {frame}"
                        )));
                    }
                    last_frame_counter = frame;
                    frames_seen += 1;
                    let icount = seg
                        .counter
                        .read()
                        .map_err(|e| BoundaryError::Exit(format!("counter read: {e:?}")))?;
                    for (slot, scheduled) in frame_inputs.iter().enumerate() {
                        if frame_inputs_applied[slot] || scheduled.frame != frame {
                            continue;
                        }
                        let boundary = Boundary {
                            icount,
                            rip: 0,
                            rcx: 0,
                        };
                        let vectors = input_sink(scheduled.index, boundary)?;
                        if !vectors.is_empty() {
                            return Err(BoundaryError::Exit(format!(
                                "frame-scheduled input for FRAME_COUNTER {frame} queued IRQ vectors; frame IRQ delivery is not wired"
                            )));
                        }
                        frame_inputs_applied[slot] = true;
                    }
                    if frame_target == Some(frames_seen) {
                        event_stop = true;
                        return Err(BoundaryError::Exit("frame budget reached".into()));
                    }
                }
                if let Some((cell, baseline)) = sdk_feed {
                    if cell.get() > baseline {
                        event_stop = true;
                        return Err(BoundaryError::Exit("sdk event matched".into()));
                    }
                }
                Ok(())
            }
        };
    }

    let mut delivered = 0u64;
    let mut timer_fired: Option<TimerFired> = None;

    // One unwind policy for every guest-executing flight: terminal HLT
    // wins (execution ended), then the until-event, else the real error
    // wrapped by `$wrap`. The flags — not the error the sentinel rode in
    // on — are the truth.
    macro_rules! unwind_or {
        ($res:expr, $wrap:expr) => {
            match $res {
                Ok(v) => v,
                Err(_) if halted => {
                    return finish_at_counter(
                        seg,
                        clock,
                        StopReason::GuestHalted,
                        delivered,
                        timer_fired,
                        frames_seen,
                    )
                }
                Err(_) if event_stop => {
                    return finish_at_counter(
                        seg,
                        clock,
                        event_reason,
                        delivered,
                        timer_fired,
                        frames_seen,
                    )
                }
                Err(e) => return Err($wrap(e)),
            }
        };
    }

    for point in &agenda {
        let boundary = unwind_or!(
            land_at(
                &mut seg.slot.vcpu,
                seg.counter,
                point.icount,
                &margins,
                exits!(),
            ),
            RunError::Boundary
        );

        // Scheduled inputs and injections at this point (§3.4). Inputs land
        // first (payload state/logging), and any edge IRQ vectors they queue
        // are delivered before precomputed vectors such as timers. KVM holds
        // ONE queued vector, and a second KVM_INTERRUPT before the next entry
        // silently OVERWRITES it (review, live-proven) — so between vectors
        // sharing a boundary the guest is entered for exactly one retirement,
        // delivering the queued vector before the next is queued ("chained
        // across consecutive entries", agenda docs).
        let mut at = boundary;
        let mut vectors: Vec<(u8, Option<usize>)> = Vec::new();
        for idx in &point.inputs {
            let queued = unwind_or!(input_sink(*idx, boundary), RunError::Boundary);
            vectors.extend(queued.into_iter().map(|vector| (vector, None)));
        }
        vectors.extend(
            point
                .injections
                .iter()
                .map(|idx| (all_injections[*idx].vector, Some(*idx))),
        );
        // A queued or delivered vector leaves transient run-control/KVM
        // interrupt state that DHSNAP does not persist. Epoch hashes still
        // record normally, but checkpoint producers must skip these points.
        let checkpointable_epoch = vectors.is_empty();
        for (i, (vector, injection_idx)) in vectors.into_iter().enumerate() {
            if i > 0 {
                // ONE ENTRY, not one retirement: the entry delivering the
                // previously queued vector runs its whole ISR before the
                // step trap fires (delivery suppresses the single-step),
                // so a +1 landing would overshoot loudly.
                at = unwind_or!(
                    crate::boundary::step_one_entry(&mut seg.slot.vcpu, seg.counter, exits!()),
                    RunError::Boundary
                );
            }
            // An event-stop (or HLT) can also surface mid-deferral —
            // inside inject_at_boundary's stepping — wrapped in its
            // error; unwind_or!'s flags, not the wrapper, are the truth.
            let inj: Injection = unwind_or!(
                inject_at_boundary(
                    &mut seg.slot.vcpu,
                    seg.counter,
                    vector,
                    &at,
                    &margins,
                    INJECT_DEFER_BUDGET,
                    exits!(),
                ),
                RunError::Inject
            );
            delivered += 1;
            if timer_slot == injection_idx {
                timer_fired = Some(TimerFired {
                    vector: inj.vector,
                    armed_deadline_vns: seg.timer.expect("timer_slot implies timer").deadline_vns,
                    delivered_icount: inj.delivered_icount,
                });
            }
            at = Boundary {
                icount: inj.delivered_icount,
                rip: inj.delivered_rip,
                rcx: at.rcx,
            };
        }

        let vns = clock
            .vns_from_icount(point.icount)
            .ok_or(RunError::ClockOverflow)?;

        let hashed_epoch =
            point.epoch_hash && (point.final_stop.is_none() || options.hash_final_epoch);
        if hashed_epoch {
            seg.chain
                .push_final_link(seg.slot, &[], point.icount, vns)
                .map_err(|e| RunError::Kvm(format!("{e:?}")))?;
            let epoch = seg.config.epoch_len.max(1);
            let checkpointable_slot = checkpointable_epoch.then_some(&*seg.slot);
            let epoch_boundary = Boundary {
                icount: point.icount,
                rip: at.rip,
                rcx: at.rcx,
            };
            epoch_sink(
                point.icount / epoch,
                epoch_boundary,
                seg.chain.value(),
                checkpointable_slot,
            )
            .map_err(RunError::Boundary)?;
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
                timer_fired,
                hashed_epoch,
                frames_seen,
                options.hash_final_stop,
            );
        }

        if let Some(kind) = point.final_stop {
            let reason = match kind {
                StopKind::Budget => StopReason::BudgetReached,
                StopKind::HardCap => StopReason::HardCap,
            };
            return finish(
                seg,
                reason,
                boundary,
                vns,
                delivered,
                timer_fired,
                hashed_epoch,
                frames_seen,
                options.hash_final_stop,
            );
        }

        // Asynchronous Pause (§3.3): honored at deterministic points only —
        // roll FORWARD to the next epoch boundary, hash it, report Paused.
        if seg.pause.load(Ordering::Relaxed) {
            if point.epoch_hash {
                return Ok(SegmentOutcome {
                    reason: StopReason::Paused,
                    boundary,
                    vns,
                    state_hash: seg.chain.value(),
                    injections_delivered: delivered,
                    timer_fired,
                    frames_elapsed: frames_seen,
                });
            }
            let epoch = seg.config.epoch_len.max(1);
            let next_epoch = point.icount.div_ceil(epoch).max(1) * epoch;
            let b = unwind_or!(
                land_at(
                    &mut seg.slot.vcpu,
                    seg.counter,
                    next_epoch,
                    &margins,
                    exits!(),
                ),
                RunError::Boundary
            );
            let vns = clock
                .vns_from_icount(b.icount)
                .ok_or(RunError::ClockOverflow)?;
            seg.chain
                .push_final_link(seg.slot, &[], b.icount, vns)
                .map_err(|e| RunError::Kvm(format!("{e:?}")))?;
            // The roll-forward lands ON the epoch grid by construction —
            // this link IS an epoch hash (b.icount is a grid multiple) —
            // but only EpochsOn configs normally RECORD epoch hashes. The
            // paranoid audit option deliberately overrides that for soak
            // runs that want every full-memory link observable.
            if epoch_hashes_enabled {
                epoch_sink(b.icount / epoch, b, seg.chain.value(), Some(&*seg.slot))
                    .map_err(RunError::Boundary)?;
            }
            return Ok(SegmentOutcome {
                reason: StopReason::Paused,
                boundary: b,
                vns,
                state_hash: seg.chain.value(),
                injections_delivered: delivered,
                timer_fired,
                frames_elapsed: frames_seen,
            });
        }
    }
    unreachable!("agenda always carries exactly one final stop point");
}

#[allow(clippy::too_many_arguments)]
fn finish(
    seg: &mut Segment<'_>,
    reason: StopReason,
    boundary: Boundary,
    vns: u64,
    delivered: u64,
    timer_fired: Option<TimerFired>,
    already_hashed: bool,
    frames_elapsed: u64,
    hash_final_stop: bool,
) -> Result<SegmentOutcome, RunError> {
    // Every segment ends with a hash link at its stop boundary (§8.5: "at
    // every final pause") — exactly ONE link per boundary: a stop point
    // that is also an epoch-hash point was linked in the walk already.
    if hash_final_stop && !already_hashed {
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
        timer_fired,
        frames_elapsed,
    })
}

/// Stop where the guest stopped, reading the boundary off the counter
/// and registers: terminal HLT (GuestHalted) and the event-driven stops
/// (the Nth frame mark / the matching SDK event), whose triggering exit
/// unwound the flight mid-air — guest-initiated at a deterministic
/// icount, so the boundary replays identically.
fn finish_at_counter(
    seg: &mut Segment<'_>,
    clock: crate::vt::ClockRatio,
    reason: StopReason,
    delivered: u64,
    timer_fired: Option<TimerFired>,
    frames_elapsed: u64,
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
        reason,
        boundary,
        vns,
        delivered,
        timer_fired,
        false,
        frames_elapsed,
        true,
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
                timer: None,
                pause: &pause,
                sdk_events: None,
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
            timer: None,
            pause: &pause,
            sdk_events: None,
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
            timer: None,
            pause: &pause,
            sdk_events: None,
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
    fn paranoid_hash_forces_epoch_grid_under_final_only_live() {
        let Some((mut slot, counter)) = rig() else {
            return;
        };
        let mut config = test_config();
        config.hash_epochs = crate::config::HashEpochs::FinalOnly;
        config.epoch_len = 50_000;
        let epoch = config.epoch_len;
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
        };
        let mut epochs = Vec::new();
        let out = run_segment_with_epoch_options(
            &mut seg,
            Until::IcountBudget(3 * epoch),
            RunOptions {
                paranoid_hash: true,
                ..RunOptions::default()
            },
            &mut never,
            &mut no_exits,
            &mut |idx, boundary, _value, _slot| {
                epochs.push((idx, boundary.icount));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::BudgetReached);
        assert_eq!(
            epochs,
            vec![(1, epoch), (2, 2 * epoch), (3, 3 * epoch)],
            "paranoid audit mode must force full-memory epoch links even under FinalOnly"
        );
    }

    #[test]
    fn pause_at_paranoid_epoch_does_not_double_hash_live() {
        let Some((mut slot, counter)) = rig() else {
            return;
        };
        let mut config = test_config();
        config.hash_epochs = crate::config::HashEpochs::FinalOnly;
        config.epoch_len = 50_000;
        let epoch = config.epoch_len;
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(true);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
        };
        let mut epochs = Vec::new();
        let out = run_segment_with_epoch_options(
            &mut seg,
            Until::IcountBudget(3 * epoch),
            RunOptions {
                paranoid_hash: true,
                ..RunOptions::default()
            },
            &mut never,
            &mut no_exits,
            &mut |idx, boundary, _value, _slot| {
                epochs.push((idx, boundary.icount));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::Paused);
        assert_eq!(out.boundary.icount, epoch);
        assert_eq!(
            epochs,
            vec![(1, epoch)],
            "pause at an already-hashed epoch must not emit a duplicate link"
        );
    }

    #[test]
    fn next_sdk_event_without_a_feed_fails_loudly() {
        // NextSdkEvent is caller-fed; running it without the feed must
        // err loudly instead of spinning to the cap with a wrong reason.
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
            timer: None,
            pause: &pause,
            sdk_events: None,
        };
        assert!(matches!(
            run_segment(
                &mut seg,
                Until::NextSdkEvent { hard_cap: 100_000 },
                &mut never,
                &mut no_exits
            ),
            Err(RunError::MissingSdkEventFeed)
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
            timer: None,
            pause: &pause,
            sdk_events: None,
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

    /// Resume-after-HLT across SEGMENTS (iteration-82 review): the
    /// batched-guest pattern (entropy_draw HLTs every batch and the
    /// harness re-runs) rests on KVM advancing RIP past the HLT before
    /// the exit, so the next segment resumes AFTER it — pin that at the
    /// VMM layer. pipeline_smoke parks in a hlt spin: every re-entered
    /// segment must halt again, strictly further along, never fault or
    /// re-deliver stale state.
    #[test]
    fn segments_resume_past_a_halt_live() {
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

        let config = MachineConfig::new(
            16 << 20,
            [0; 32],
            crate::config::BootSpec::Elf {
                kernel_hash: [0; 32],
                cmdline: Vec::new(),
            },
        );
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut last_icount = 0u64;
        for round in 0..3 {
            let start = counter.read().unwrap();
            let mut seg = Segment {
                slot: &mut slot,
                counter: &counter,
                chain: &mut chain,
                config: &config,
                start_icount: start,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: None,
            };
            let out = run_segment(
                &mut seg,
                Until::IcountBudget(1_000_000),
                &mut || false,
                &mut |exit| {
                    if let VcpuExit::IoOut(port, _) = exit {
                        if (0x3F8..0x400).contains(&port) {
                            return Ok(());
                        }
                    }
                    Err(BoundaryError::Exit(format!("unexpected: {exit:?}")))
                },
            )
            .unwrap();
            assert_eq!(out.reason, StopReason::GuestHalted, "round {round}");
            assert!(
                out.boundary.icount > last_icount,
                "round {round}: did not resume past the previous halt \
                 ({} <= {last_icount})",
                out.boundary.icount
            );
            last_icount = out.boundary.icount;
        }
    }
}

#[cfg(test)]
mod timer_tests {
    use super::*;
    use crate::boot::load_and_enter;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;
    use crate::vt::ClockRatio;
    use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};

    #[test]
    fn conversion_follows_the_ceil_rule_and_clamps() {
        // 1:1 — icount == vns.
        let c11 = ClockRatio::new(1, 1).unwrap();
        let inj = timer_to_injection(
            TimerArm {
                deadline_vns: 5_000,
                vector: 0x40,
            },
            c11,
            0,
        )
        .unwrap();
        assert_eq!(
            inj,
            ScheduledInjection {
                icount: 5_000,
                vector: 0x40
            }
        );

        // 2 vns per instruction: deadline 9 vns -> ceil(9/2) = 5 instr.
        let c21 = ClockRatio::new(2, 1).unwrap();
        assert_eq!(
            timer_to_injection(
                TimerArm {
                    deadline_vns: 9,
                    vector: 1
                },
                c21,
                0
            )
            .unwrap()
            .icount,
            5
        );

        // A deadline at/before the segment start clamps to start + 1.
        assert_eq!(
            timer_to_injection(
                TimerArm {
                    deadline_vns: 10,
                    vector: 1
                },
                c11,
                10_000
            )
            .unwrap()
            .icount,
            10_001
        );
    }

    #[test]
    fn linux_cpu_compat_timer_conversion_is_counter_space_and_deterministic() {
        let clock = ClockRatio::new(3, 2).unwrap();
        let arm = TimerArm {
            deadline_vns: 10,
            vector: 0x41,
        };
        let a = timer_to_injection(arm, clock, 0).unwrap();
        let b = timer_to_injection(arm, clock, 0).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a,
            ScheduledInjection {
                icount: 7,
                vector: 0x41
            },
            "ceil(10 * 2 / 3) = 7 guest instructions"
        );
        assert_eq!(
            timer_to_injection(arm, clock, 20).unwrap().icount,
            21,
            "expired timers clamp to the next guest-instruction boundary"
        );
    }

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    /// The full guest-armed chain, live: deadline vns -> ceil icount ->
    /// merged agenda point -> §3.4 queue (one deferral step refreshes the
    /// stale exit-time IF summary) -> TimerFired reported with the AUX
    /// record's exact fields. The IDT-equipped delivery observation is
    /// bead 583's guest.
    #[test]
    fn armed_timer_fires_and_reports_live() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, nanokernel::landing_loop_elf(), b"1000000000").unwrap();
        // Test-only: open the interrupt window (the landing loop never
        // does STI; entry rflags has IF clear).
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rflags |= 1 << 9;
        slot.vcpu.set_regs(&regs).unwrap();

        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();

        let config = MachineConfig::new(
            16 << 20,
            [0; 32],
            crate::config::BootSpec::Elf {
                kernel_hash: [0; 32],
                cmdline: Vec::new(),
            },
        );
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        const DEADLINE: u64 = 123_456; // 1:1 clock -> icount 123_456
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: Some(TimerArm {
                deadline_vns: DEADLINE,
                vector: 0x40,
            }),
            pause: &pause,
            sdk_events: None,
        };
        // Budget EQUAL to the deadline: injection and final stop merge
        // into one agenda point, so the queued vector never enters the
        // empty IDT and the outcome returns cleanly. (The landing's own
        // stepping refreshes kvm_run's IF summary, so the window is
        // already open at the boundary — no deferral step.)
        let out = run_segment(
            &mut seg,
            Until::IcountBudget(DEADLINE),
            &mut || false,
            &mut |exit| Err(BoundaryError::Exit(format!("unexpected: {exit:?}"))),
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::BudgetReached);
        let fired = out.timer_fired.expect("timer must have fired");
        assert_eq!(fired.vector, 0x40);
        assert_eq!(fired.armed_deadline_vns, DEADLINE);
        // The landing's stepping refreshed the IF summary: queued at the
        // exact converted boundary, no deferral.
        assert_eq!(fired.delivered_icount, DEADLINE);
        assert_eq!(out.injections_delivered, 1);
    }
}

#[cfg(test)]
mod idt_guest_tests {
    use super::*;
    use crate::boot::load_and_enter;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;
    use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
    use vm_memory::Bytes;

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    fn rig(cmdline: &[u8]) -> Option<(crate::kvm::SlotVm, InstRetired)> {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return None;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, nanokernel::timer_guest_elf(), cmdline).unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();
        Some((slot, counter))
    }

    fn test_cfg() -> MachineConfig {
        MachineConfig::new(
            16 << 20,
            [0; 32],
            crate::config::BootSpec::Elf {
                kernel_hash: [0; 32],
                cmdline: Vec::new(),
            },
        )
    }

    fn read_table(slot: &crate::kvm::SlotVm) -> (u64, Vec<u8>) {
        let mut head = [0u8; 8];
        slot.guest_mem
            .read_slice(
                &mut head,
                vm_memory::GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA),
            )
            .unwrap();
        let count = u64::from_le_bytes(head);
        let mut vecs = vec![0u8; count as usize];
        slot.guest_mem
            .read_slice(
                &mut vecs,
                vm_memory::GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA + 8),
            )
            .unwrap();
        (count, vecs)
    }

    fn no_exits(exit: VcpuExit) -> Result<(), BoundaryError> {
        Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}")))
    }

    /// THE iter-35 chain coverage: two vectors scheduled at ONE boundary
    /// must BOTH deliver — observed in the guest's ISR table, in order.
    #[test]
    fn two_vectors_at_one_boundary_both_deliver_live() {
        let run = || {
            let (mut slot, counter) = rig(b"")?;
            let cfg = test_cfg();
            let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
            let pause = AtomicBool::new(false);
            let injections = [
                ScheduledInjection {
                    icount: 50_000,
                    vector: 0x40,
                },
                ScheduledInjection {
                    icount: 50_000,
                    vector: 0x41,
                },
            ];
            let mut seg = Segment {
                slot: &mut slot,
                counter: &counter,
                chain: &mut chain,
                config: &cfg,
                start_icount: 0,
                injections: &injections,
                timer: None,
                pause: &pause,
                sdk_events: None,
            };
            let out = run_segment(
                &mut seg,
                Until::IcountBudget(80_000),
                &mut || false,
                &mut no_exits,
            )
            .unwrap();
            assert_eq!(out.reason, StopReason::BudgetReached);
            assert_eq!(out.injections_delivered, 2);
            let (count, vecs) = read_table(seg.slot);
            assert_eq!(count, 2, "BOTH handlers must have run (no overwrite)");
            assert_eq!(vecs, vec![0x40, 0x41], "delivery order is schedule order");
            Some((out.boundary.rip, out.state_hash, vecs))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "two-vector delivery must replay identically");
    }

    /// The armed-timer chain with a REAL ISR: TimerFired reported AND the
    /// handler observably ran.
    #[test]
    fn timer_delivery_runs_the_isr_live() {
        let Some((mut slot, counter)) = rig(b"") else {
            return;
        };
        let cfg = test_cfg();
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &cfg,
            start_icount: 0,
            injections: &[],
            timer: Some(TimerArm {
                deadline_vns: 60_000,
                vector: 0x41,
            }),
            pause: &pause,
            sdk_events: None,
        };
        let out = run_segment(
            &mut seg,
            Until::IcountBudget(100_000),
            &mut || false,
            &mut no_exits,
        )
        .unwrap();
        let fired = out.timer_fired.expect("timer fired");
        assert_eq!(fired.vector, 0x41);
        let (count, vecs) = read_table(seg.slot);
        assert_eq!((count, vecs), (1, vec![0x41]), "the ISR recorded delivery");
    }

    /// The masked variant: IF stays 0 across the deadline — §3.4 deferral
    /// hits the bounded loud stop, deterministically.
    #[test]
    fn masked_variant_defers_forever_live() {
        let Some((mut slot, counter)) = rig(b"mask") else {
            return;
        };
        let cfg = test_cfg();
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let injections = [ScheduledInjection {
            icount: 30_000,
            vector: 0x40,
        }];
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &cfg,
            start_icount: 0,
            injections: &injections,
            timer: None,
            pause: &pause,
            sdk_events: None,
        };
        let err = run_segment(
            &mut seg,
            Until::IcountBudget(80_000),
            &mut || false,
            &mut no_exits,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                RunError::Inject(crate::inject::InjectError::WindowNeverOpened { .. })
            ),
            "masked guest must defer to the bound: {err:?}"
        );
        let (count, _) = read_table(seg.slot);
        assert_eq!(count, 0, "no delivery can have happened with IF=0");
    }
}

#[cfg(test)]
mod event_until_tests {
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

    fn rig(elf: &[u8], cmdline: &[u8]) -> Option<(crate::kvm::SlotVm, InstRetired)> {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return None;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, elf, cmdline).unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();
        Some((slot, counter))
    }

    fn cfg() -> MachineConfig {
        MachineConfig::new(
            16 << 20,
            [0; 32],
            crate::config::BootSpec::Elf {
                kernel_hash: [0; 32],
                cmdline: Vec::new(),
            },
        )
    }

    /// Host-runnable (no KVM): the frame-mark GPA runctl decodes must
    /// stay the pv-pad window slot the guests write — a silent device
    /// constant move that still lands inside the tests' MMIO window
    /// would otherwise pass the live tests while breaking real rails.
    #[test]
    fn frame_mark_gpa_is_pinned_to_the_device_window() {
        assert_eq!(
            dh_devices::pad::PV_PAD_BASE + dh_devices::pad::REG_FRAME_COUNTER,
            0xD000_101C
        );
    }

    /// pad_echo's exit surface: pv-pad MMIO (PAD0 polls answered 0,
    /// FRAME_COUNTER writes accepted) + debug serial.
    fn pad_serial_exits(exit: VcpuExit) -> Result<(), BoundaryError> {
        const PAD_LO: u64 = dh_devices::pad::PV_PAD_BASE;
        const PAD_HI: u64 = PAD_LO + 0x1000;
        match exit {
            VcpuExit::MmioWrite(gpa, _) if (PAD_LO..PAD_HI).contains(&gpa) => Ok(()),
            VcpuExit::MmioRead(gpa, data) if (PAD_LO..PAD_HI).contains(&gpa) => {
                data.fill(0);
                Ok(())
            }
            VcpuExit::IoOut(port, _) if (0x3F8..0x400).contains(&port) => Ok(()),
            other => Err(BoundaryError::Exit(format!("unexpected exit: {other:?}"))),
        }
    }

    /// API.md §2.4: frame_budget stops ON the frame-boundary exit of the
    /// Nth FrameMark, reason BUDGET_REACHED — and the boundary replays
    /// identically (the exit is guest-initiated at a fixed icount).
    #[test]
    fn frame_budget_stops_on_the_nth_frame_mark_live() {
        let run = || {
            let (mut slot, counter) = rig(nanokernel::pad_echo_elf(), b"")?;
            let config = cfg();
            let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
            let pause = AtomicBool::new(false);
            let mut seg = Segment {
                slot: &mut slot,
                counter: &counter,
                chain: &mut chain,
                config: &config,
                start_icount: 0,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: None,
            };
            let out = run_segment(
                &mut seg,
                Until::FrameBudget {
                    frames: 3,
                    hard_cap: 50_000_000,
                },
                &mut || false,
                &mut pad_serial_exits,
            )
            .unwrap();
            assert_eq!(out.reason, StopReason::BudgetReached);
            assert_eq!(out.frames_elapsed, 3);
            assert!(out.boundary.icount > 0);
            Some((out.boundary.icount, out.boundary.rip, out.state_hash))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "frame-budget stop must replay identically");
    }

    #[test]
    fn frame_scheduled_input_lands_on_matching_frame_mark_live() {
        let Some((mut slot, counter)) = rig(nanokernel::pad_echo_elf(), b"") else {
            return;
        };
        let config = cfg();
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
        };
        let frame_inputs = [ScheduledFrameInput { frame: 2, index: 0 }];
        let mut landed = Vec::new();
        let out = run_segment_with_scheduled_inputs_and_frames(
            &mut seg,
            Until::FrameBudget {
                frames: 3,
                hard_cap: 50_000_000,
            },
            &[0],
            &frame_inputs,
            0,
            &mut || false,
            &mut pad_serial_exits,
            &mut |idx, boundary| {
                landed.push((idx, boundary.icount, boundary.rip));
                Ok(Vec::new())
            },
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::BudgetReached);
        assert_eq!(out.frames_elapsed, 3);
        assert_eq!(landed.len(), 1);
        assert_eq!(landed[0].0, 0);
        assert!(landed[0].1 > 0);
        assert_eq!(landed[0].2, 0, "exit-time rail boundary_rip convention");
        assert!(
            landed[0].1 <= out.boundary.icount,
            "input must land no later than the frame-budget stop"
        );
    }

    /// FrameBudget(0) — zero MORE frames — is satisfied at the start
    /// boundary without entering the guest (IcountBudget(0) semantics).
    #[test]
    fn frame_budget_zero_stops_at_the_start_boundary_live() {
        let Some((mut slot, counter)) = rig(nanokernel::pad_echo_elf(), b"") else {
            return;
        };
        let config = cfg();
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
        };
        let out = run_segment(
            &mut seg,
            Until::FrameBudget {
                frames: 0,
                hard_cap: 1_000_000,
            },
            &mut || false,
            &mut pad_serial_exits,
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::BudgetReached);
        assert_eq!(out.boundary.icount, 0, "no guest entry");
        assert_eq!(out.frames_elapsed, 0);
    }

    /// A guest that never writes FRAME_COUNTER runs to the request's
    /// safety net: HARD_CAP, zero frames elapsed.
    #[test]
    fn frame_budget_without_frames_hits_the_hard_cap_live() {
        let Some((mut slot, counter)) = rig(nanokernel::landing_loop_elf(), b"1000000000") else {
            return;
        };
        let config = cfg();
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
        };
        let out = run_segment(
            &mut seg,
            Until::FrameBudget {
                frames: 1,
                hard_cap: 300_000,
            },
            &mut || false,
            &mut |exit| Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}"))),
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::HardCap);
        assert_eq!(out.boundary.icount, 300_000);
        assert_eq!(out.frames_elapsed, 0);
    }

    /// NextSdkEvent stops ON the exit whose servicing reported a
    /// matching event (here: the caller bumps the feed at the guest's
    /// first serial OUT — the rail does the same at a matching doorbell
    /// drain), reason NEXT_SDK_EVENT, replay-identical boundary.
    #[test]
    fn next_sdk_event_stops_at_the_feeding_exit_live() {
        let run = || {
            let (mut slot, counter) = rig(nanokernel::pipeline_smoke_elf(), b"")?;
            let config = cfg();
            let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
            let pause = AtomicBool::new(false);
            let events = std::cell::Cell::new(0u64);
            let mut seg = Segment {
                slot: &mut slot,
                counter: &counter,
                chain: &mut chain,
                config: &config,
                start_icount: 0,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: Some(&events),
            };
            let out = run_segment(
                &mut seg,
                Until::NextSdkEvent {
                    hard_cap: 1_000_000,
                },
                &mut || false,
                &mut |exit| match exit {
                    VcpuExit::IoOut(port, _) if (0x3F8..0x400).contains(&port) => {
                        events.set(events.get() + 1);
                        Ok(())
                    }
                    other => Err(BoundaryError::Exit(format!("unexpected exit: {other:?}"))),
                },
            )
            .unwrap();
            assert_eq!(out.reason, StopReason::NextSdkEvent);
            assert!(
                out.boundary.icount < 1_000_000,
                "must stop at the event, not the cap"
            );
            // Cross-mode frame counting must not leak into a non-frame
            // run (pipeline_smoke never writes FRAME_COUNTER).
            assert_eq!(out.frames_elapsed, 0);
            Some((out.boundary.icount, out.boundary.rip, out.state_hash))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "sdk-event stop must replay identically");
    }

    /// No matching event all segment: the safety net stops it (HARD_CAP),
    /// never a spin.
    #[test]
    fn next_sdk_event_without_events_hits_the_hard_cap_live() {
        let Some((mut slot, counter)) = rig(nanokernel::landing_loop_elf(), b"1000000000") else {
            return;
        };
        let config = cfg();
        let mut chain = StateHashChain::new(&[1; 32], &[2; 32]);
        let pause = AtomicBool::new(false);
        let events = std::cell::Cell::new(0u64);
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: Some(&events),
        };
        let out = run_segment(
            &mut seg,
            Until::NextSdkEvent { hard_cap: 300_000 },
            &mut || false,
            &mut |exit| Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}"))),
        )
        .unwrap();
        assert_eq!(out.reason, StopReason::HardCap);
        assert_eq!(out.boundary.icount, 300_000);
    }
}
