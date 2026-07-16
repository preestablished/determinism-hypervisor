# Critical And Important Findings

No critical findings.

## Important: The new early-HLT hash-substitution case is unreachable in the replay path

The new helper only allows substituting the recorded `end_state_hash` when the tail stopped with `GuestHalted` in the half-open range `terminal_event_icount..end_icount`:

- `crates/dh-worker/src/replay_engine.rs:602`
- `crates/dh-worker/src/replay_engine.rs:611`
- `crates/dh-worker/src/replay_engine.rs:613`

The production call passes `target.icount` as `terminal_event_icount`:

- `crates/dh-worker/src/replay_engine.rs:2046`
- `crates/dh-worker/src/replay_engine.rs:2049`

But `terminal_sdk_target_for_tail` can only construct a target by finding an SDK event whose recorded icount equals `end_icount`, then returning that same icount:

- `crates/dh-worker/src/replay_engine.rs:2232`
- `crates/dh-worker/src/replay_engine.rs:2254`
- `crates/dh-worker/src/replay_engine.rs:2258`
- `crates/dh-worker/src/replay_engine.rs:2259`
- `crates/dh-worker/src/replay_engine.rs:2261`

So, in the actual replay path, the helper receives `terminal_event_icount == end_icount`, making `terminal_event_icount..end_icount` empty. The positive cases added in `terminal_sdk_end_hash_substitution_is_limited_to_early_hlt_tail` use hard-coded `12, 20` inputs:

- `crates/dh-worker/src/replay_engine.rs:2699`
- `crates/dh-worker/src/replay_engine.rs:2701`
- `crates/dh-worker/src/replay_engine.rs:2704`
- `crates/dh-worker/src/replay_engine.rs:2709`
- `crates/dh-worker/src/replay_engine.rs:2712`

Those inputs do not match what `terminal_sdk_target_for_tail` can currently return for a terminal SDK target.

Impact: the branch narrows the old broad substitution, but the intended Linux terminal-tail accommodation described in the comments is not actually preserved for an early `GuestHalted` tail before the recorded snapshot boundary. The same target-icount issue also feeds `terminal_sdk_finish_tail_matches_recording` through the tail check:

- `crates/dh-worker/src/replay_engine.rs:1953`
- `crates/dh-worker/src/replay_engine.rs:1957`
- `crates/dh-worker/src/replay_engine.rs:1958`
- `crates/dh-worker/src/replay_engine.rs:599`

With the current detector, an early HLT before `end_icount` is below the accepted range as well. If the intended behavior is to allow "terminal SDK event regenerated earlier inside the final epoch, then HLT before BudgetReached boundary", this branch is too narrow and will likely report `end_state_hash` divergence instead of normalizing that specific case.

Recommended fix: make the replay path pass a reachable lower-bound icount for the normalized terminal tail, or change the target detector/predicate pair so the recorded terminal target and the allowed early-HLT boundary are consistent. Add a regression that derives the target through `terminal_sdk_target_for_tail` rather than calling the helper with hand-picked `12, 20` values.
