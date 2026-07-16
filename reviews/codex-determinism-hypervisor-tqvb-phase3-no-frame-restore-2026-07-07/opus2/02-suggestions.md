# Suggestions (non-blocking) — 2nd reviewer

### S1. Narrow start-of-run race in the live-inject fast path

**File:** `crates/dh-worker/src/service.rs:4498-4509` (InjectInputs handler).

`InjectInputs` checks `live.with_active(...)`; if it returns `None` (no active
streaming run) it falls through to `with_runtime_mut(... queue_inputs_from_proto)`,
which routes over the actor command channel. If a `RunWithFrameCapture` *starts*
in the window between `with_active` returning `None` and the `with_runtime_mut`
send landing, that `InjectInputs` will queue behind the whole play session on the
actor channel — the exact head-of-line block M3 exists to avoid, just in a tiny
window. Not a correctness bug (it eventually runs, validated against the
then-current state), but it can hang an `InjectInputs` RPC for a full session.
Consider re-checking `with_active` inside the actor closure, or documenting the
window as accepted.

### S2. `at_frame` acceptance floor is the run's internal frame, not the last frame the consumer saw

**File:** `crates/dh-worker/src/runtime.rs:444-508` (`LiveInputRun.last_streamed_frame`,
`observe_frame`) + `service.rs:242` (`live_inject_from_proto` validation).

`observe_frame` runs in `live_input_sink` *before* `frame_sink` emits the frame,
and updates `last_streamed_frame` to the frame the run has reached. With channel
capacity 2 the run can be up to ~2 frames ahead of what the consumer has actually
received, so an operator who just saw frame `N` and injects at `N+1` may get
`INVALID_ARGUMENT` because `last_streamed_frame` is already `N+2`. This is
*correct* for determinism (you cannot inject into a frame whose boundary the run
has passed) and it fails loudly, never silently — but the field name
`last_streamed_frame` implies "last frame streamed to the consumer" when it is
"highest frame the run has reached." Consider renaming to `last_reached_frame` /
`highest_run_frame` to prevent a future maintainer from "fixing" the floor toward
the consumer's position and reintroducing a determinism hazard.

### S3. Live leftovers re-queue as static frame inputs (exact `==` match), losing the `<=` catch-up semantics

**File:** `crates/dh-worker/src/service.rs:786-802` (leftover re-queue) vs
`runtime.rs:499-505` (`observe_frame` uses `target <= frame`) and
`runctl.rs:659` (static frame input uses `scheduled.frame != frame`).

An accepted-but-not-yet-due live input that the run never reached is re-queued
into `runtime.queued_inputs` as a `Frame(target)` input. On the next run it is
serviced by the static frame-input path, which matches the target frame value
*exactly* — so if that frame is ever skipped (a FRAME_COUNTER jump > 1; the
monotonic check only requires strictly-increasing), the input is silently
dropped, whereas the live path would have caught it at the next boundary via
`<=`. Frame skips are unusual with pv-pad, so this is an edge case, but the
asymmetry means "accepted is never dropped" (the module doc's promise) is only
strictly true within the same run. Worth a one-line comment acknowledging the
downgrade, or re-queueing with a small note that catch-up semantics do not carry
across the run boundary.

### S4. `frame_emit_duration_milliseconds` help text overstates what it measures

**File:** `crates/dh-worker/src/service.rs:88-96` (help text) vs `:1346`/`:1361`
(`emit_start` / `record_streamed_frame`).

`emit_start` is captured at hook entry, i.e. *after* the framebuffer read and lz4
compression, which happen in `frame_sink` inside `drive_recorded_run`. So the
metric measures only stream-send + backpressure hold, but the help string says
"framebuffer read, lz4, stream send incl. backpressure hold." Either move the
`Instant::now()` to the start of `frame_sink` (before the fb read) to match the
text, or trim the help string to "stream send incl. backpressure hold."

### S5. M9 budget constants are workload-drift-brittle

**Files:** `crates/dh-worker/tests/play_perf_smoke.rs:31`
(`FRAME_HARD_CAP = 50_000_000`), `frame_capture_stream.rs:24,27,391`
(`NEUTRALITY_BUDGET`, `SHORT_BUDGET`, `12 * 28_000_000`).

At the measured ~27.8M instr/frame these carry acceptable headroom, but they are
implicit multiples of a measured constant. If per-frame cost drifts up (e.g.
toward 50M) the per-frame `Run{frame_budget=1}` in `play_perf_smoke` stops at
`HARD_CAP` mid-frame and `instr_per_frame` becomes meaningless, and the
live-inject test's `12 * 28_000_000` budget may fail to reach the injected frame.
Both fail loudly (not silently), and these are `#[ignore]` operator tests, so
this is low-severity — but consider deriving the budgets from a single named
`INSTR_PER_FRAME_ESTIMATE` constant with an explicit headroom multiplier so drift
is adjusted in one place, and assert `reason == BUDGET_REACHED` on each per-frame
`Run` in Phase A so a capped frame fails clearly rather than skewing the ratio.

**Research ref:** `rust-integration-testing.md` — "Are failure paths ... covered";
brittle constants should fail with a clear message.

### S6. `RunWithFrameCapture` / M3 integration is exercised only by `#[ignore]` M9 tests

**Files:** `crates/dh-worker/tests/frame_capture_stream.rs`,
`play_perf_smoke.rs` (all `#[ignore]`).

The checkout_write interplay (InjectInputs succeeding against a `RUNNING` slot
mid-stream), the actor-channel bypass, cancel landing, and watchdog are only
covered by KVM+artifact-gated tests that do not run in normal CI. The
host-runnable unit coverage (`live_inject_from_proto` validation, the runctl
frame-sink neutrality/stop tests) is good, but the service-layer wiring is
untested off-M9. Consider a lightweight host-level test of the
`FrameSinkFlow`/hook state machine (channel-full → hold → `Closed` → `Stop`
cause = "cancel"; watchdog deadline → `Stop` cause = "watchdog"; terminal
delivery) using a fake sink, independent of KVM — this is where I1 would have
been caught. Matches the existing M9-gating convention, so non-blocking.
