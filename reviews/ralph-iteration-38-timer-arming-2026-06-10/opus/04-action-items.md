# Action Items

Self-contained list. Verdict is APPROVE; nothing here blocks merge.

### Critical

_None._

### Important

_None._

### Suggestions

- **[S1] Document the silent beyond-segment timer exclusion.** In
  `crates/dh-vmm/src/runctl.rs`, extend the `timer_to_injection` doc comment to state
  that `Ok(_)` does not guarantee the timer fires this segment: if the converted/clamped
  icount exceeds the segment's final stop, `agenda::compile` excludes it and
  `timer_fired` is `None`. This is correct one-shot behavior (the device is disarmed only
  on fire, so the still-armed deadline is re-derived against the next segment's
  `vns_base`), but the silence is non-obvious from `timer_to_injection` alone. Doc-only.

- **[S2] Add a host-side test for the beyond-budget non-fire path.** A `run_segment`
  test with `timer.deadline_vns` past a short `IcountBudget` asserting `timer_fired ==
  None` and `injections_delivered == 0` would lock in the semantics reasoned in 01 Walk B
  and guard a future agenda-window refactor. Cheap; not blocking.

- **[S3] Watch `finish`'s argument count.** The 7-arg `finish` with
  `#[allow(clippy::too_many_arguments)]` is fine for now. If a second result field is
  threaded through the finish paths later (e.g. a device-event delivery record),
  introduce a small `SegmentResult` aggregate then. Do not refactor for the single
  current field.

### Forward-looking semantics note (for the M6 scheduler bead, not this iteration)

- **Cross-segment queued-undelivered vector.** In the budget == deadline case the live
  test exploits, `KVM_INTERRUPT(v)` queues the vector but the segment returns before the
  next `KVM_RUN` entry — leaving a latent pending vector in KVM that would be delivered on
  the *next* segment's first entry. The test ends at the segment boundary, so it is fine
  here. But run-control *composition* (chaining segments on the same vCPU slot) must
  account for a queued-undelivered vector crossing the segment boundary: the next
  segment's `start_icount` already has an interrupt pending, and the AUX `TIMER_FIRE`
  record's `delivered_icount` (the queue boundary) precedes the handler's actual entry in
  the following segment. Capture this as a precondition / invariant in the M6 scheduler
  bead so the composition layer either drains the queue at segment end or carries the
  pending state forward deterministically. Flagged per the review brief; no code change in
  iteration 38.
