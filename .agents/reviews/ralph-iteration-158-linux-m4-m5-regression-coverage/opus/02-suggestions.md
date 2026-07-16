# 02-suggestions.md

Suggestion: `crates/dh-worker/tests/m5_frame_scheduling.rs:247` accepts `HardCap`, `BudgetReached`, or `GuestHalted`. That is honest for the current fixture, but the no-frame path should also prove the queued `AtFrame` input was not consumed early. Recommended fix: after a `HardCap`/`GuestHalted` result with `frames_elapsed == 0`, take/seal a snapshot and assert the DHILOG contains no `PadSet` for the frame-scheduled input.

Suggestion: `crates/dh-worker/tests/m5_frame_scheduling.rs:281` uses `saturating_sub` for `delta_icount`. This can hide a bad cumulative-icount regression as `0`. Recommended fix: use `checked_sub` and fail if `run.icount < ready_icount`.

Suggestion: `crates/dh-worker/tests/m4_transparency.rs:462` names the Linux check as transparency, but it only proves restore/fork at READY plus snapshot identity, not resumed execution after restore. Recommended fix: add a brief comment near the test explaining this is the current fixture-limited Linux M4 replacement, not the full landing-loop M4 property.
