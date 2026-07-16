# Critical And Important Findings

## Important: early terminal SDK HLT accommodation is unreachable through the production target path

`terminal_sdk_target_for_tail` only returns a `ReplaySdkEventTarget` after finding an SDK event whose recorded icount equals `end_icount` (`crates/dh-worker/src/replay_engine.rs:2232`-`2263`). The returned `target.icount` therefore equals `header.end_icount` for every target selected by the production call at `crates/dh-worker/src/replay_engine.rs:1283`.

The terminal tail acceptance path then passes that recorded target icount into `terminal_sdk_finish_tail_matches_recording` (`crates/dh-worker/src/replay_engine.rs:1954`-`1960`). For a recorded `BudgetReached` segment that replays as an early `GuestHalted` tail, the helper requires `finish_icount` to fall in `(terminal_event_icount..=end_icount)` (`crates/dh-worker/src/replay_engine.rs:597`-`599`). Since `terminal_event_icount == end_icount` in the integrated path, any genuinely early halt at `< end_icount` is rejected before the new hash-normalization guard runs.

The new substitution guard has the same mismatch. It passes `target.icount` into `terminal_sdk_recorded_end_hash_substitution_allowed` (`crates/dh-worker/src/replay_engine.rs:2046`-`2052`), while that helper requires `out.boundary.icount` to be in the half-open range `(terminal_event_icount..end_icount)` (`crates/dh-worker/src/replay_engine.rs:611`-`614`). With equal bounds, the range is empty, so substitution is never allowed for targets selected by `terminal_sdk_target_for_tail`.

The new unit test covers the helper with a synthetic `terminal_event_icount` of `12` and `end_icount` of `20` (`crates/dh-worker/src/replay_engine.rs:2699`-`2706`), but that value cannot be produced by the current `terminal_sdk_target_for_tail` selection for a terminal target. As a result, the branch removes the broad exact-end masking, but it does not preserve the intended Linux terminal SDK early-HLT tail accommodation described by the bead.

Recommended fix: track the icount where the matching SDK event is observed during replay and use that observed icount for the early-tail lower bound, or change the guard to rely on "matching target was observed" plus `expected_reason == BudgetReached`, `tail.reason == GuestHalted`, and `tail.boundary.icount < header.end_icount`. Keep exact-end `BudgetReached` and exact-end `GuestHalted` tails outside substitution so final-state divergence remains visible.

