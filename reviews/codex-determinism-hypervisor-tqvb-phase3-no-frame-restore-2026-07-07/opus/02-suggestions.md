# Suggestions (non-blocking)

### S1. `record_frame_stream_termination` only `debug_assert!`s label membership

**File:** `crates/dh-worker/src/service.rs:362-369`, list at `322-331`.

`render` iterates the fixed `FRAME_STREAM_TERMINATION_LABELS`, so a reason that
is not in the list is counted into the `BTreeMap` but never rendered — silently
lost in release builds, where the `debug_assert!` is compiled out. Every current
call site uses an in-list literal, so this is latent, but it is brittle: adding a
new `stop_cause` string or a new stop-reason arm without updating the list drops
the metric. Consider a release-safe fallback so unknown labels are never lost:

```rust
fn record_frame_stream_termination(&self, reason: &'static str) {
    let reason = if FRAME_STREAM_TERMINATION_LABELS.contains(&reason) {
        reason
    } else {
        debug_assert!(false, "unlisted frame-stream termination label: {reason}");
        "other"
    };
    *self.frame_stream_terminations.lock().expect("...").entry(reason).or_insert(0) += 1;
}
```

### S2. Backpressure `try_send` + `sleep(1ms)` busy-poll

**File:** `crates/dh-worker/src/service.rs:5159-5194`.

The manual `try_send`/`sleep(Duration::from_millis(1))` loop is deliberate (it
keeps the per-frame watchdog and keeps the block on the actor thread, not the
runtime), and matches the guidance in
`tokio-channel-streaming-deadlocks.md` about avoiding `.send().await`. It is
correct. The only downside is up to ~1ms of added latency per backpressured
frame and a wakeup per ms while held. This is acceptable for a viewer-paced
stream; noting only so a future reader does not "optimize" it into an
`.await`-based send (which would move the block onto a runtime worker and
reintroduce the deadlock surface the design avoids). A short comment to that
effect near the loop would help.

### S3. `hard_icount_cap` silently ignored by `RunWithFrameCapture`

**File:** `crates/dh-worker/src/service.rs:1426-1449`.

`until_from_frame_capture_request` maps only the two budget arms and never reads
`hard_icount_cap` (documented as "unused by the budget modes, mirroring
`until_from_run_request`"). This is internally consistent, but the proto field
is still accepted and the tests pass `hard_icount_cap: 0`. Consider either
documenting in the proto that the field is ignored for this RPC, or (cheaply)
rejecting a nonzero value with `InvalidArgument` so a caller who sets it does not
assume a safety net that is not wired.

### S4. `drive_recorded_run` length / nesting

**File:** `crates/dh-worker/src/service.rs:3242-3745`.

The function is ~500 lines with several deeply nested closures (`epoch_sink`'s
bisection-checkpoint block is itself ~120 lines at `3408-3530`). It is correct
and well-commented, but extracting the checkpoint-capture body into a named
helper taking a small context struct would make the frame-capture additions
(`frame_sink`, `live_input_sink`) easier to read and reduce the
`#[allow(clippy::too_many_arguments)]` surface. Non-blocking.

### S5. Detached frames thread has no lifecycle tracking

**File:** `crates/dh-worker/src/service.rs:5133-5258`.

The `dh-frames-{slot}` thread is spawned and its `JoinHandle` dropped. Combined
with I1, there is no way to observe or reap a stuck orchestration thread. Even
without adopting I1's timeout, a `dh_worker_frame_orchestration_threads` gauge
(inc on spawn, dec at end) would surface the leak in metrics. Non-blocking.

### S6. `replay_engine.rs` final-link condition now reads the live counter

**File:** `crates/dh-worker/src/replay_engine.rs:2071-2078`.

The change from `last_canonical_icount == Some(header.end_icount)` to
`counter.read()? == header.end_icount` for pushing the segment-final device
link is determinism-sensitive (it decides whether the terminal link is emitted).
It is the right fix for the no-doorbell frame-boundary case where the run
physically reaches `end_icount` past the last canonical record, and it is
guarded by the store-joint / m5 replay-equality tests. Worth an explicit
one-line comment stating *why* the physical counter, not the last canonical
record, is authoritative here, so the invariant is not "simplified" back later.
</content>
