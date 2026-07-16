# Critical & Important Findings — 2nd reviewer

## Critical

**None.** The hard constraints all hold on inspection:

- **State-hash chain unchanged.** `run_segment_with_frame_captures` threads the
  identical `epoch_sink` / `hash_final_stop` / pause-roll-forward logic as the
  pre-existing `run_segment_with_scheduled_inputs_frames_and_epochs`. The frame
  sink and live-input sink are inserted at the FRAME_MARK exit *inside* `on_exit`
  servicing (`runctl.rs:678-698`) and never call `push_final_link` or
  `epoch_sink`. Links stay on the epoch grid + final stop. The
  `FrameSinkFlow::Stop` landing routes through `finish_at_counter` →
  `finish(..., hash_final_stop)` producing exactly the one final link a
  `FrameBudget` stop at that frame produces (asserted by
  `frame_sink_stop_lands_paused_at_the_frame_boundary_live`, runctl.rs:2191).
- **Backpressure blocks only the actor thread.** `drive_recorded_run` runs on
  the slot's dedicated actor OS thread (routed via `with_runtime_mut` →
  `SlotActor::with_runtime_mut`, service.rs:3216 / runtime.rs:240); the hook's
  `try_send` + `sleep(1ms)` loop (service.rs:1355-1390) executes inside that
  closure, so the hold is on the actor thread. The `dh-frames-N` std thread only
  parks on `rx.recv()`.
- **Capture neutrality.** The frame sink does a read-only framebuffer extract;
  the detchannel frame-boundary drain (service.rs:2298) runs in the shared
  `service_exit_with_detchannel` for both plain `Run` and streaming, and is
  mirrored in `replay_engine.rs`, so a capture run and a no-capture run over the
  same budget drain identically (asserted by leg-1-vs-leg-2 of
  `linux_streaming_capture_is_neutral_complete_and_backpressure_safe`).

## Important

### I1. Terminal `blocking_send` can strand the `dh-frames-N` thread after the watchdog fires — partially defeating the watchdog

**File:** `crates/dh-worker/src/service.rs:1446-1453` (terminal send), interacting
with `:1327` (`FRAME_STREAM_CHANNEL_CAPACITY = 2`) and `:1370-1377` (watchdog).

**Description.** The stalled-consumer watchdog exists precisely because "a
consumer that keeps the connection open but stops reading would otherwise hold
the vCPU, the slot's actor thread, and its pinned core forever" (const doc,
service.rs:1195-1200). When it fires, the hook returns `Stop`, `drive_recorded_run`
returns, and the actor thread / pinned core / vCPU are correctly freed. But the
channel (capacity 2) is still full of unread frames because the consumer is not
reading, and the handler then does:

```rust
let _ = tx.blocking_send(Ok(proto::FrameCaptureEvent {
    msg: Some(proto::frame_capture_event::Msg::Done(done)),
}));
```

`blocking_send` parks until a slot frees. A consumer that holds the gRPC
connection open but never reads again keeps the two buffered frames in place, so
this `blocking_send` blocks **for the lifetime of the connection** — leaking the
`dh-frames-N` thread. This is the exact scenario the watchdog defends against;
the watchdog frees the three named resources but not this thread. (Per the
tokio-channel research: a bounded-channel producer that sends beyond capacity
with nothing draining "deadlocks itself — silently, with no error and no CPU.")

The M9 test does not catch it because
`linux_stalled_consumer_watchdog_ends_the_run_paused` (frame_capture_stream.rs:573)
*resumes reading* after 35s, which drains the buffer and lets `blocking_send`
complete. A never-resuming consumer is untested.

**Suggested fix.** Bound the terminal delivery so a stalled consumer cannot pin
the thread. Either a timed send:

```rust
// The run already ended; if the consumer is still not reading, don't
// pin this thread — the stream tears down when tonic drops rx.
match result {
    Ok(done) => {
        let _ = tx.blocking_send_timeout(
            Ok(FrameCaptureEvent { msg: Some(Msg::Done(done)) }),
            FRAME_STREAM_STALL_WATCHDOG, // or a small fixed bound
        );
    }
    Err(e) => { let _ = tx.blocking_send_timeout(Err(e), ...); }
}
```

(`tokio::sync::mpsc::Sender` has no `blocking_send_timeout`; use
`rt_handle.block_on(tokio::time::timeout(dur, tx.send(msg)))` on a small helper
runtime, or a `try_send` retry loop bounded by the same watchdog constant.)
Whatever the mechanism, the terminal send should not be able to outlive the
watchdog by more than a bounded margin. A never-reading consumer that later
resumes would then observe a truncated stream (buffered frames, then close with
no `Done`) — which is acceptable because it violated the read contract; a
slow-but-live consumer is unaffected.

**Research ref:** `tokio-channel-streaming-deadlocks.md` — "a producer that sends
more items than the channel capacity deadlocks itself — silently, with no error
and no CPU."

### I2. Replay segment-final-link condition was rewritten to read the counter — verify it does not shift a hash link for `next_sdk_event` / terminal-target replays

**File:** `crates/dh-worker/src/replay_engine.rs:2071-2081` (and the removed
`last_canonical_icount` tracking at :1837, :1955).

**Description.** The condition guarding the segment-final `push_final_link` at
replay end changed from

```rust
if terminal_sdk_target.is_none()
    && tail.is_none()
    && last_canonical_icount == Some(header.end_icount)
    && last_epoch_icount.get() != Some(header.end_icount)
```

to

```rust
let replay_counter_at_end = counter.read()? == header.end_icount;
if tail.is_none() && replay_counter_at_end
    && last_epoch_icount.get() != Some(header.end_icount)
```

Two behavioral changes: (a) the source of truth moved from "last canonical
record icount" to "the live counter", and (b) the `terminal_sdk_target.is_none()`
guard was dropped. For a replay whose recorded segment stopped on a
`next_sdk_event` (terminal target `Some`), the old code suppressed the final link
here; the new code will push it whenever the counter sits at `end_icount` and no
epoch link already landed there. Since this is the replay/verify rail that must
reproduce the recorded chain **bit-for-bit**, an extra or shifted final link
would surface as a spurious `Divergence`. This is part of the earlier
frame-boundary-drain commit (4b19c52), not the M2/M3 core, and the live-inject
M9 test does exercise a full `VerifyReplay` round-trip — but only for
budget-terminated segments, not for a `next_sdk_event`-terminated one.

**Suggested fix.** Confirm (with a targeted replay test, or by argument in the
commit message) that no recorded segment terminating on `next_sdk_event` reaches
this branch with `counter == end_icount` and no epoch link — i.e. that dropping
`terminal_sdk_target.is_none()` is genuinely inert. If it is not provably inert,
restore the terminal-target guard.

### I3. `frame_holds_in_progress` gauge strands at a nonzero value if the actor thread panics mid-hold

**File:** `crates/dh-worker/src/service.rs:47-59` (`frame_hold_started` /
`frame_hold_finished`) and the hook at `:1366-1388`.

**Description.** In the normal paths the gauge is balanced: `frame_hold_started`
is called once (via `get_or_insert_with`) on entering the `Full` state, and
`frame_hold_finished` is called on all three hook exits (`Ok`, watchdog `Stop`,
`Closed`). But `frame_hold_started` decrements/increments run on the **actor
thread** (inside `drive_recorded_run`'s frame sink → hook). If that thread panics
while a hold is in progress (e.g. a KVM/model panic surfacing between
`frame_hold_started` and the next `try_send`), the actor thread dies without
`frame_hold_finished`, and the gauge is stuck at +1 permanently — and unlike the
counters, a gauge that only climbs is misleading for capacity dashboards. This is
an edge case (actor-thread panic is already catastrophic for the slot), but the
gauge is the one metric that cannot self-correct.

**Suggested fix.** Prefer a RAII guard that decrements on drop, so unwind through
the hold restores the gauge:

```rust
struct HoldGuard<'a>(&'a WorkerMetrics);
impl Drop for HoldGuard<'_> { fn drop(&mut self) { self.0.frame_hold_finished_gauge_only(); } }
```

or, more cheaply, document that this gauge is best-effort and reset it to
`frame_stream` liveness on the next successful stream start. Low priority.

**Research ref:** `rust-tracing-observability.md` — "Ensure buffer capacity is
documented and intentional"; gauges must not leak on the error path.
