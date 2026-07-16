# Positive notes

### P1 — Genuinely live, genuinely deterministic
The headline claim holds. `dh-cli run landing_loop.elf --icount-budget 500000` run twice
produced byte-identical JSON **including the state_hash**
(`5398e78d…baa06fbf`), and all 4 runctl unit tests are real `/dev/kvm` runs (not mocked).
The run-twice-compare pattern is baked into the test itself
(`icount_budget_runs_twice_identically_live` compares the full outcome tuple), which is
exactly the right discipline for a determinism platform.

### P2 — Until/StopReason enums map 1:1 onto API.md §2.4
`Until::{IcountBudget, VnsBudget, Goal, NextSdkEvent, FrameBudget}` mirrors the proto
`oneof until` (icount_budget/vns_budget/goal/next_sdk_event/frame_budget), and
`StopReason::{BudgetReached, GoalSatisfied, HardCap, Paused}` is a clean subset of the
proto enum. Keeping the unwired variants *in the enum* (returning `NotYetWired`) rather
than omitting them means M6's gRPC Run maps without reshaping the type — the right call,
and `unwired_modes_fail_loudly` pins it with a test.

### P3 — Margins sourced from MachineConfig (bead srz, done right)
`Margins { skid_margin, resync_slack }` is built from `seg.config` (lines 150-153), not
hardcoded — the single-source-of-truth the srz bead asked for. And the landing knobs are
correctly excluded from the config preimage (verified in config.rs
`landing_knobs_do_not_fork_identity`), so tuning them never forks snapshot identity.

### P4 — Pause roll-forward arithmetic is correct and well-guarded
`point.icount.div_ceil(epoch).max(1) * epoch` is right at every corner: exact-multiple
points land in place (no Overshoot), non-multiples advance to the next grid line, and
`.max(1)` guards icount 0. The `pause_rolls_forward_to_the_epoch_boundary_live` test
asserts both the multiple-of-epoch landing and the ≤ epoch_len latency bound. (The only
gap is the un-clamped overrun past the budget — S1 — not the math.)

### P5 — Goal-before-stop ordering is the right precedence
At a coincident goal-poll + hard-cap point, GoalSatisfied is checked first (line 225)
and wins over HardCap (line 229). Since `hard_icount_cap` is a *safety net* (API.md §2.4),
reporting GOAL_SATISFIED when the goal genuinely holds at the cap boundary is the more
informative and correct reason. Defensible and consistent with the proto's intent.

### P6 — Agenda compilation is exemplary
`agenda.rs` is pure, overflow-checked at the u64 edge (the `grid_point_at_u64_max`
regression test and the 2000-case `prop_*` property test that includes an edge regime),
and the merge semantics (coincident actions fold into one StopPoint) exactly implement
§3.3's `merge(...)`. The field docs nail the subtle distinctions: epoch grid
SEGMENT-aligned, goal polls RUN-relative, injection indices in canonical recorded order
for replay identity. This is the kind of layer that makes the run loop above it simple.

### P7 — Honest, accurate module docs (one exception)
The runctl and inject module headers are unusually precise — inject.rs even documents the
KVM_INTERRUPT overwrite footgun and the IrqWindowOpen-vs-Debug deferral hazard with
"live-reproduced in review" provenance. The single inaccurate comment (the multi-vector
chaining claim, C1) stands out precisely *because* the rest is so faithful; it reads like
an intention that was never wired, not sloppiness.
