//! Agenda compilation (ARCH §3.3): `Run(until)` compiles to a sorted agenda
//! of stop points — scheduled injections, epoch boundaries, goal polls, and
//! the final stop. Every agenda point is a pure function of the inputs, so
//! two replays compute identical agendas.
//!
//! Dynamic stops (`next_sdk_event` doorbell checks, `frame_budget` Nth
//! frame-boundary exit) are flags consumed by run control, NOT agenda
//! entries — they stop the run when the guest-initiated exit happens, which
//! is itself at a deterministic icount.

use crate::vt::ClockRatio;
use std::num::NonZeroU64;

/// The `Run.until` budget that determines the final stop (API.md §2.4).
/// Budgets are relative: "run this many MORE instructions / this much MORE
/// virtual time" from the current position. Goal / SDK-event / frame-budget
/// runs bound the segment with `hard_icount_cap` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalStop {
    /// `icount_budget`: stop after this many more instructions.
    IcountBudget(u64),
    /// `vns_budget`: stop at the first icount where the segment's vns has
    /// advanced by at least this much (converted via the clock rational).
    VnsBudget(u64),
    /// Goal / next_sdk_event / frame_budget runs: the safety-net cap.
    HardCap(u64),
}

/// Why the agenda's last point stops the run — preserved so run control can
/// report the API's distinct stop reasons (BUDGET_REACHED vs the hard-cap
/// safety net) without re-deriving them from the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopKind {
    /// An icount/vns budget was exhausted → BUDGET_REACHED.
    Budget,
    /// The `hard_icount_cap` safety net for goal/sdk/frame waits.
    HardCap,
}

/// Everything the compiler needs. `start_icount` is segment-relative (icount
/// is 0 at segment start, ARCH §8.1); a Run resuming after a mid-segment
/// pause starts where the pause left off.
#[derive(Clone, Debug)]
pub struct AgendaInputs<'a> {
    /// Current position (segment-relative). The agenda covers (start, final].
    pub start_icount: u64,
    /// Scheduled input landing points, segment-relative icounts. Entries
    /// outside (start, final] stay pending with the caller; they are not this
    /// agenda's business.
    ///
    /// ORDER CONTRACT mirrors `injections`: `StopPoint::inputs` stores indices
    /// into this slice, so same-boundary inputs land in caller-provided order.
    pub scheduled_inputs: &'a [u64],
    /// Scheduled injection points (precomputed vectors plus pv timer arms),
    /// segment-relative icounts. Entries outside (start, final] stay pending
    /// with the caller; they are not this agenda's business.
    ///
    /// ORDER CONTRACT: `StopPoint::injections` stores indices into this
    /// slice, so replay identity requires both runs to present injections in
    /// the same, canonical order — the DHILOG order they were recorded in.
    /// "Same multiset, different order" is NOT replay-identical.
    pub injections: &'a [u64],
    /// Epoch boundary spacing — hash points at every MULTIPLE of `epoch_len`
    /// since segment start (verify mode: always on; normal mode:
    /// configurable, default on). `None` disables epoch hashing. Segment
    /// alignment means a pause/resume never shifts the hash grid.
    pub epoch_len: Option<NonZeroU64>,
    /// Goal poll spacing; `Some` iff `Run.until` carries a goal condition.
    /// Polls are RUN-relative — the first poll is exactly `goal_poll_period`
    /// instructions after `start_icount` (API.md §2.4: "instructions between
    /// polls"), unlike the segment-aligned epoch grid. Deterministic either
    /// way (the Run boundary itself is part of the replayed timeline).
    pub goal_poll_period: Option<NonZeroU64>,
    pub final_stop: FinalStop,
    /// The VM's fixed clock rational (needed for vns budgets).
    pub clock: ClockRatio,
}

/// One stop point. Coincident actions merge: a single landing can inject,
/// hash, poll, and stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopPoint {
    pub icount: u64,
    /// Indices into `AgendaInputs::scheduled_inputs` that land here,
    /// ascending.
    pub inputs: Vec<usize>,
    /// Indices into `AgendaInputs::injections` that fire here, ascending.
    /// More than one index means multiple injections share this boundary;
    /// ARCH §3.4 delivers ONE vector per VM entry, so run control must chain
    /// them across consecutive entries at this boundary (its concern, not
    /// the agenda's) — but the set and order here are replay-stable.
    pub injections: Vec<usize>,
    pub epoch_hash: bool,
    pub goal_poll: bool,
    /// `Some` on exactly one point per agenda — the last one.
    pub final_stop: Option<StopKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgendaError {
    /// Budget arithmetic exceeded u64 icount space.
    BudgetOverflow,
}

/// Compile the sorted agenda. Pure: identical inputs yield identical output.
pub fn compile(inputs: &AgendaInputs<'_>) -> Result<Vec<StopPoint>, AgendaError> {
    let start = inputs.start_icount;
    let (final_icount, stop_kind) = match inputs.final_stop {
        FinalStop::IcountBudget(b) => (
            start.checked_add(b).ok_or(AgendaError::BudgetOverflow)?,
            StopKind::Budget,
        ),
        FinalStop::HardCap(b) => (
            start.checked_add(b).ok_or(AgendaError::BudgetOverflow)?,
            StopKind::HardCap,
        ),
        FinalStop::VnsBudget(b) => {
            // First icount whose vns reaches vns(start) + b. Both conversions
            // are the §4 pure functions, so this is replay-stable.
            let start_vns = inputs
                .clock
                .vns_from_icount(start)
                .ok_or(AgendaError::BudgetOverflow)?;
            let target_vns = start_vns
                .checked_add(b)
                .ok_or(AgendaError::BudgetOverflow)?;
            let target = inputs
                .clock
                .icount_for_vns_target(target_vns)
                .ok_or(AgendaError::BudgetOverflow)?;
            // target < start happens only for zero/sub-instruction budgets
            // (vns(start) truncates downward, and the ceil conversion of that
            // truncated value can land at or before start). Clamping to
            // start means "stop at the current boundary without executing".
            (target.max(start), StopKind::Budget)
        }
    };

    let mut points: Vec<StopPoint> = Vec::new();
    let mut push = |icount: u64, f: &dyn Fn(&mut StopPoint)| match points
        .binary_search_by_key(&icount, |p| p.icount)
    {
        Ok(i) => f(&mut points[i]),
        Err(i) => {
            let mut p = StopPoint {
                icount,
                inputs: Vec::new(),
                injections: Vec::new(),
                epoch_hash: false,
                goal_poll: false,
                final_stop: None,
            };
            f(&mut p);
            points.insert(i, p);
        }
    };

    // Final stop first — it defines the agenda's end and always exists.
    push(final_icount, &|p| p.final_stop = Some(stop_kind));

    // Scheduled inputs within (start, final].
    for (idx, &at) in inputs.scheduled_inputs.iter().enumerate() {
        if at > start && at <= final_icount {
            push(at, &|p| p.inputs.push(idx));
        }
    }

    // Scheduled injections within (start, final].
    for (idx, &at) in inputs.injections.iter().enumerate() {
        if at > start && at <= final_icount {
            push(at, &|p| p.injections.push(idx));
        }
    }

    // Epoch grid: multiples of epoch_len from SEGMENT start (icount 0)
    // within (start, final]. The checked k step matters: a grid point can
    // legitimately land on u64::MAX, where an unchecked k+1 would overflow.
    if let Some(len) = inputs.epoch_len {
        let len = len.get();
        let mut k = start / len + 1;
        while let Some(at) = k.checked_mul(len) {
            if at > final_icount {
                break;
            }
            push(at, &|p| p.epoch_hash = true);
            let Some(next) = k.checked_add(1) else { break };
            k = next;
        }
    }

    // Goal polls: RUN-relative — start + k*period, k >= 1 (see field doc).
    if let Some(period) = inputs.goal_poll_period {
        let period = period.get();
        let mut at = start.checked_add(period);
        while let Some(p_at) = at {
            if p_at > final_icount {
                break;
            }
            push(p_at, &|p| p.goal_poll = true);
            at = p_at.checked_add(period);
        }
    }

    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    fn nz(v: u64) -> NonZeroU64 {
        NonZeroU64::new(v).unwrap()
    }

    fn inputs<'a>(injections: &'a [u64]) -> AgendaInputs<'a> {
        AgendaInputs {
            start_icount: 0,
            scheduled_inputs: &[],
            injections,
            epoch_len: Some(nz(1000)),
            goal_poll_period: None,
            final_stop: FinalStop::IcountBudget(10_000),
            clock: ClockRatio::default(),
        }
    }

    #[test]
    fn merges_all_sources_sorted() {
        let agenda = compile(&AgendaInputs {
            start_icount: 0,
            scheduled_inputs: &[1500, 3000],
            injections: &[1500, 500, 3000],
            epoch_len: Some(nz(1000)),
            goal_poll_period: Some(nz(1500)),
            final_stop: FinalStop::IcountBudget(3000),
            clock: ClockRatio::default(),
        })
        .unwrap();

        let icounts: Vec<u64> = agenda.iter().map(|p| p.icount).collect();
        assert_eq!(icounts, vec![500, 1000, 1500, 2000, 3000]);

        // 1500: injection idx 0 + goal poll (run-relative: 0 + 1*1500).
        let p1500 = &agenda[2];
        assert_eq!(p1500.inputs, vec![0]);
        assert_eq!(p1500.injections, vec![0]);
        assert!(p1500.goal_poll && !p1500.epoch_hash && p1500.final_stop.is_none());

        // 3000: injection idx 2 + epoch + poll + final all coincide.
        let p3000 = agenda.last().unwrap();
        assert_eq!(p3000.inputs, vec![1]);
        assert_eq!(p3000.injections, vec![2]);
        assert!(p3000.epoch_hash && p3000.goal_poll);
        assert_eq!(p3000.final_stop, Some(StopKind::Budget));
    }

    #[test]
    fn stop_kind_distinguishes_budget_from_hard_cap() {
        for (fs, kind) in [
            (FinalStop::IcountBudget(100), StopKind::Budget),
            (FinalStop::VnsBudget(100), StopKind::Budget),
            (FinalStop::HardCap(100), StopKind::HardCap),
        ] {
            let a = compile(&AgendaInputs {
                final_stop: fs,
                ..inputs(&[])
            })
            .unwrap();
            assert_eq!(a.last().unwrap().final_stop, Some(kind), "{fs:?}");
        }
    }

    #[test]
    fn injections_outside_window_excluded() {
        let agenda = compile(&AgendaInputs {
            start_icount: 2000,
            injections: &[2000, 1999, 2001, 12_000, 12_001],
            ..inputs(&[])
        })
        .unwrap();
        // (start, final] = (2000, 12000]: at-start and past-final excluded.
        let injected: Vec<u64> = agenda
            .iter()
            .filter(|p| !p.injections.is_empty())
            .map(|p| p.icount)
            .collect();
        assert_eq!(injected, vec![2001, 12_000]);
    }

    #[test]
    fn epoch_grid_aligns_to_segment_polls_align_to_run() {
        let a = compile(&AgendaInputs {
            start_icount: 1500,
            goal_poll_period: Some(nz(1000)),
            final_stop: FinalStop::IcountBudget(2500),
            ..inputs(&[])
        })
        .unwrap();
        let hashes: Vec<u64> = a
            .iter()
            .filter(|p| p.epoch_hash)
            .map(|p| p.icount)
            .collect();
        assert_eq!(hashes, vec![2000, 3000, 4000]); // segment multiples of 1000
        let polls: Vec<u64> = a.iter().filter(|p| p.goal_poll).map(|p| p.icount).collect();
        assert_eq!(
            polls,
            vec![2500, 3500],
            "first poll one full period after run start"
        );
    }

    #[test]
    fn grid_point_at_u64_max_no_overflow() {
        // Regression: a grid multiple landing exactly on u64::MAX must not
        // overflow the k counter (debug panic / release wraparound).
        let a = compile(&AgendaInputs {
            start_icount: u64::MAX - 2,
            scheduled_inputs: &[],
            injections: &[],
            epoch_len: Some(nz(1)),
            goal_poll_period: Some(nz(1)),
            final_stop: FinalStop::IcountBudget(2),
            clock: ClockRatio::default(),
        })
        .unwrap();
        let icounts: Vec<u64> = a.iter().map(|p| p.icount).collect();
        assert_eq!(icounts, vec![u64::MAX - 1, u64::MAX]);
        assert!(a.iter().all(|p| p.epoch_hash && p.goal_poll));
        assert_eq!(a.last().unwrap().final_stop, Some(StopKind::Budget));
    }

    #[test]
    fn vns_budget_converts_via_clock() {
        let clock = ClockRatio::new(1, 3).unwrap(); // 3 instructions per vns
        let agenda = compile(&AgendaInputs {
            start_icount: 10,
            scheduled_inputs: &[],
            injections: &[],
            epoch_len: None,
            goal_poll_period: None,
            final_stop: FinalStop::VnsBudget(5),
            clock,
        })
        .unwrap();
        // vns(10) = 3; target vns 8 ⇒ final = ceil(8*3/1) = 24.
        assert_eq!(agenda.len(), 1);
        assert_eq!(agenda[0].icount, 24);
        assert_eq!(agenda[0].final_stop, Some(StopKind::Budget));
    }

    #[test]
    fn zero_budget_stops_at_start() {
        for fs in [FinalStop::IcountBudget(0), FinalStop::VnsBudget(0)] {
            let agenda = compile(&AgendaInputs {
                start_icount: 777,
                final_stop: fs,
                ..inputs(&[])
            })
            .unwrap();
            assert_eq!(agenda.len(), 1);
            assert_eq!(agenda[0].icount, 777, "{fs:?}");
            assert!(agenda[0].final_stop.is_some());
        }
    }

    #[test]
    fn errors_are_loud() {
        assert_eq!(
            compile(&AgendaInputs {
                start_icount: u64::MAX - 5,
                final_stop: FinalStop::IcountBudget(10),
                ..inputs(&[])
            }),
            Err(AgendaError::BudgetOverflow)
        );
        let fast = ClockRatio::new(u32::MAX, 1).unwrap();
        assert_eq!(
            compile(&AgendaInputs {
                start_icount: u64::MAX / 2,
                scheduled_inputs: &[],
                injections: &[],
                epoch_len: None,
                goal_poll_period: None,
                final_stop: FinalStop::VnsBudget(0),
                clock: fast,
            }),
            Err(AgendaError::BudgetOverflow) // vns(start) itself overflows
        );
    }

    #[test]
    fn prop_pure_sorted_unique_bounded_and_replay_identical() {
        let mut rng = XorShift(0x2545F4914F6CDD1D);
        for case in 0..2000u32 {
            // Every 8th case runs at the u64 edge to exercise the overflow
            // regime; the rest stay in the readable small domain.
            let edge = case % 8 == 0;
            let start = if edge {
                u64::MAX - 2 - rng.next() % 1_000
            } else {
                rng.next() % 1_000_000
            };
            let budget = if edge {
                rng.next() % (u64::MAX - start)
            } else {
                rng.next() % 1_000_000
            };
            let injections: Vec<u64> = (0..rng.next() % 20)
                .map(|_| {
                    if edge {
                        start.saturating_add(rng.next() % 2_000)
                    } else {
                        rng.next() % 2_000_000
                    }
                })
                .collect();
            let scheduled_inputs: Vec<u64> = (0..rng.next() % 20)
                .map(|_| {
                    if edge {
                        start.saturating_add(rng.next() % 2_000)
                    } else {
                        rng.next() % 2_000_000
                    }
                })
                .collect();
            let in1 = AgendaInputs {
                start_icount: start,
                scheduled_inputs: &scheduled_inputs,
                injections: &injections,
                epoch_len: (!rng.next().is_multiple_of(4)).then(|| nz(1 + rng.next() % 50_000)),
                goal_poll_period: (rng.next().is_multiple_of(2))
                    .then(|| nz(1 + rng.next() % 50_000)),
                final_stop: match rng.next() % 3 {
                    0 => FinalStop::IcountBudget(budget),
                    1 if !edge => FinalStop::VnsBudget(budget),
                    _ => FinalStop::HardCap(budget),
                },
                clock: ClockRatio::new(
                    1 + (rng.next() % 1000) as u32,
                    1 + (rng.next() % 1000) as u32,
                )
                .unwrap(),
            };
            let a = compile(&in1).unwrap();
            // Replay-identical: a second compile of the same inputs is equal.
            assert_eq!(a, compile(&in1).unwrap());

            // Sorted strictly ascending (unique icounts).
            assert!(a.windows(2).all(|w| w[0].icount < w[1].icount));

            // Non-empty; last point is the final stop and the ONLY final
            // stop; every point lies in (start, final] (or == start for the
            // zero-budget degenerate case).
            let last = a.last().unwrap();
            assert!(last.final_stop.is_some());
            assert_eq!(a.iter().filter(|p| p.final_stop.is_some()).count(), 1);
            for p in &a {
                assert!(p.icount >= start && p.icount <= last.icount);
                assert!(p.icount > start || (p.icount == start && p.final_stop.is_some()));
                assert!(
                    !p.injections.is_empty()
                        || !p.inputs.is_empty()
                        || p.epoch_hash
                        || p.goal_poll
                        || p.final_stop.is_some()
                );
                // Injection indices are ascending (input order).
                assert!(p.inputs.windows(2).all(|w| w[0] < w[1]));
                assert!(p.injections.windows(2).all(|w| w[0] < w[1]));
            }

            // Every in-window scheduled input appears exactly once, at its icount.
            for (idx, &at) in scheduled_inputs.iter().enumerate() {
                let hits: Vec<&StopPoint> =
                    a.iter().filter(|p| p.inputs.contains(&idx)).collect();
                if at > start && at <= last.icount {
                    assert_eq!(hits.len(), 1);
                    assert_eq!(hits[0].icount, at);
                } else {
                    assert!(hits.is_empty());
                }
            }

            // Every in-window injection appears exactly once, at its icount.
            for (idx, &at) in injections.iter().enumerate() {
                let hits: Vec<&StopPoint> =
                    a.iter().filter(|p| p.injections.contains(&idx)).collect();
                if at > start && at <= last.icount {
                    assert_eq!(hits.len(), 1);
                    assert_eq!(hits[0].icount, at);
                } else {
                    assert!(hits.is_empty());
                }
            }

            // Epoch grid: exactly the segment multiples of epoch_len in
            // window. Goal polls: exactly start + k*period in window.
            if let Some(len) = in1.epoch_len {
                let len = len.get();
                for p in &a {
                    assert_eq!(p.epoch_hash, p.icount % len == 0 && p.icount > start);
                }
            }
            if let Some(period) = in1.goal_poll_period {
                let period = period.get();
                for p in &a {
                    let rel = p.icount - start;
                    assert_eq!(p.goal_poll, rel > 0 && rel % period == 0);
                }
            }
        }
    }
}
