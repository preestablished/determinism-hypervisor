# Overview

Reviewed branch `ralph/iteration-159-audit-terminal-detchannel-replay-end-has` against `main` for bead `determinism-hypervisor-j71`.

Scope reviewed:

- `crates/dh-worker/src/replay_engine.rs`
- checkpoint commit `044016a ralph: iteration 159 checkpoint - narrow terminal sdk hash normalization`

The branch changes terminal SDK replay end-hash handling from unconditional substitution whenever a terminal SDK target exists to a narrower helper that only substitutes the recorded `end_state_hash` for an early `GuestHalted` tail under a recorded `BudgetReached` end. That direction is aligned with the bead: exact-end tails should not get a recorded hash injected over a real final-state mismatch.

I found one important integration issue: the new helper is correct in isolation, but the production value passed as `terminal_event_icount` is always the recorded `end_icount` for terminal SDK targets selected by `terminal_sdk_target_for_tail`. That makes the early-`GuestHalted` accommodation unreachable in the current integrated path.

