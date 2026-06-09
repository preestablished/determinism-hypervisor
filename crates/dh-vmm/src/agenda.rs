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

/// Everything the compiler needs. `start_icount` is segment-relative (icount
/// is 0 at segment start, ARCH §8.1); a Run resuming after a mid-segment
/// pause starts where the pause left off.
#[derive(Clone, Debug)]
pub struct AgendaInputs<'a> {
    /// Current position (segment-relative). The agenda covers (start, final].
    pub start_icount: u64,
    /// Scheduled injection points (from InjectInputs + pv timer arms),
    /// segment-relative icounts, any order. Entries outside (start, final]
    /// stay pending with the caller; they are not this agenda's business.
    pub injections: &'a [u64],
    /// Epoch boundary spacing — hash points every `epoch_len` icount
    /// (verify mode: always on; normal mode: configurable, default on).
    /// `None` disables epoch hashing entirely.
    pub epoch_len: Option<u64>,
    /// Goal poll spacing; `Some` iff `Run.until` carries a goal condition.
    pub goal_poll_period: Option<u64>,
    pub final_stop: FinalStop,
    /// The VM's fixed clock rational (needed for vns budgets).
    pub clock: ClockRatio,
}

/// One stop point. Coincident actions merge: a single landing can inject,
/// hash, poll, and stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopPoint {
    pub icount: u64,
    /// Indices into `AgendaInputs::injections` that fire here (input order).
    pub injections: Vec<usize>,
    pub epoch_hash: bool,
    pub goal_poll: bool,
    pub final_stop: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgendaError {
    /// epoch_len or goal_poll_period of zero is meaningless.
    ZeroPeriod,
    /// Budget arithmetic exceeded u64 icount space.
    BudgetOverflow,
}

/// Compile the sorted agenda. Pure: identical inputs yield identical output.
pub fn compile(inputs: &AgendaInputs<'_>) -> Result<Vec<StopPoint>, AgendaError> {
    if inputs.epoch_len == Some(0) || inputs.goal_poll_period == Some(0) {
        return Err(AgendaError::ZeroPeriod);
    }

    let start = inputs.start_icount;
    let final_icount = match inputs.final_stop {
        FinalStop::IcountBudget(b) | FinalStop::HardCap(b) => {
            start.checked_add(b).ok_or(AgendaError::BudgetOverflow)?
        }
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
            // A zero-vns budget (or sub-instruction remainder) must still
            // make forward progress impossible to confuse with "no stop":
            // the final stop is never before the current position.
            target.max(start)
        }
    };

    // Collect (icount, action) pairs, then merge. Capacity guess: exact for
    // injections, grids counted below.
    let mut points: Vec<StopPoint> = Vec::new();
    let mut push = |icount: u64, f: &dyn Fn(&mut StopPoint)| match points
        .binary_search_by_key(&icount, |p| p.icount)
    {
        Ok(i) => f(&mut points[i]),
        Err(i) => {
            let mut p = StopPoint {
                icount,
                injections: Vec::new(),
                epoch_hash: false,
                goal_poll: false,
                final_stop: false,
            };
            f(&mut p);
            points.insert(i, p);
        }
    };

    // Final stop first — it defines the agenda's end and always exists.
    push(final_icount, &|p| p.final_stop = true);

    // Scheduled injections within (start, final].
    for (idx, &at) in inputs.injections.iter().enumerate() {
        if at > start && at <= final_icount {
            push(at, &|p| p.injections.push(idx));
        }
    }

    // Periodic grids: multiples of the period within (start, final]. Grid
    // points are multiples of the period from SEGMENT start (icount 0), not
    // from this Run's start — so a pause/resume inside a segment never
    // shifts the hash or poll grid.
    let mut grid = |period: Option<u64>, f: &dyn Fn(&mut StopPoint)| {
        let Some(period) = period else { return };
        let mut k = start / period + 1;
        while let Some(at) = k.checked_mul(period) {
            if at > final_icount {
                break;
            }
            push(at, f);
            k += 1;
        }
    };
    grid(inputs.epoch_len, &|p| p.epoch_hash = true);
    grid(inputs.goal_poll_period, &|p| p.goal_poll = true);

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

    fn inputs<'a>(injections: &'a [u64]) -> AgendaInputs<'a> {
        AgendaInputs {
            start_icount: 0,
            injections,
            epoch_len: Some(1000),
            goal_poll_period: None,
            final_stop: FinalStop::IcountBudget(10_000),
            clock: ClockRatio::default(),
        }
    }

    #[test]
    fn merges_all_sources_sorted() {
        let agenda = compile(&AgendaInputs {
            start_icount: 0,
            injections: &[1500, 500, 3000],
            epoch_len: Some(1000),
            goal_poll_period: Some(1500),
            final_stop: FinalStop::IcountBudget(3000),
            clock: ClockRatio::default(),
        })
        .unwrap();

        let icounts: Vec<u64> = agenda.iter().map(|p| p.icount).collect();
        assert_eq!(icounts, vec![500, 1000, 1500, 2000, 3000]);

        // 1500: injection idx 0 + goal poll merge on one landing.
        let p1500 = &agenda[2];
        assert_eq!(p1500.injections, vec![0]);
        assert!(p1500.goal_poll && !p1500.epoch_hash && !p1500.final_stop);

        // 3000: injection idx 2 + epoch + poll + final all coincide.
        let p3000 = agenda.last().unwrap();
        assert_eq!(p3000.injections, vec![2]);
        assert!(p3000.epoch_hash && p3000.goal_poll && p3000.final_stop);
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
    fn grids_align_to_segment_not_run_start() {
        // Resuming mid-segment must not shift the epoch grid.
        let a = compile(&AgendaInputs {
            start_icount: 1500,
            final_stop: FinalStop::IcountBudget(2000),
            ..inputs(&[])
        })
        .unwrap();
        let hashes: Vec<u64> = a
            .iter()
            .filter(|p| p.epoch_hash)
            .map(|p| p.icount)
            .collect();
        assert_eq!(hashes, vec![2000, 3000]); // multiples of 1000, not 1500+k*1000
    }

    #[test]
    fn vns_budget_converts_via_clock() {
        let clock = ClockRatio::new(1, 3).unwrap(); // 3 instructions per vns
        let agenda = compile(&AgendaInputs {
            start_icount: 10,
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
        assert!(agenda[0].final_stop);
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
            assert!(agenda[0].final_stop);
        }
    }

    #[test]
    fn errors_are_loud() {
        assert_eq!(
            compile(&AgendaInputs {
                epoch_len: Some(0),
                ..inputs(&[])
            }),
            Err(AgendaError::ZeroPeriod)
        );
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
        for _ in 0..2000 {
            let start = rng.next() % 1_000_000;
            let injections: Vec<u64> = (0..rng.next() % 20)
                .map(|_| rng.next() % 2_000_000)
                .collect();
            let in1 = AgendaInputs {
                start_icount: start,
                injections: &injections,
                epoch_len: (rng.next() % 4 != 0).then(|| 1 + rng.next() % 50_000),
                goal_poll_period: (rng.next() % 2 == 0).then(|| 1 + rng.next() % 50_000),
                final_stop: match rng.next() % 3 {
                    0 => FinalStop::IcountBudget(rng.next() % 1_000_000),
                    1 => FinalStop::VnsBudget(rng.next() % 1_000_000),
                    _ => FinalStop::HardCap(rng.next() % 1_000_000),
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

            // Non-empty; last point is the final stop and the ONLY final stop;
            // every point lies in (start, final] (or == start for zero budget).
            let last = a.last().unwrap();
            assert!(last.final_stop);
            assert_eq!(a.iter().filter(|p| p.final_stop).count(), 1);
            for p in &a {
                assert!(p.icount >= start && p.icount <= last.icount);
                assert!(p.icount > start || (p.icount == start && p.final_stop));
                // Every point does something.
                assert!(!p.injections.is_empty() || p.epoch_hash || p.goal_poll || p.final_stop);
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

            // Epoch grid: exactly the multiples of epoch_len in window.
            if let Some(len) = in1.epoch_len {
                for p in &a {
                    assert_eq!(p.epoch_hash, p.icount % len == 0 && p.icount > start);
                }
                let mut k = start / len + 1;
                while k * len <= last.icount {
                    assert!(a.iter().any(|p| p.icount == k * len && p.epoch_hash));
                    k += 1;
                }
            }
        }
    }
}
