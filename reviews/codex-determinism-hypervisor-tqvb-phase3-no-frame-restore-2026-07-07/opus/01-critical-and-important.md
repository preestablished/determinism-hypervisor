# Critical and Important Findings

## Critical

**None.** I specifically hunted for chain/determinism violations and could not
find one:

- Chain links are pushed only in `run_segment_inner` at epoch-grid points
  (`push_final_link` under `hashed_epoch`), the pause roll-forward epoch, and
  the single final-stop link in `finish`. `frame_sink` never touches
  `seg.chain`; it is invoked between exit servicing and the frame-budget/SDK
  checks with the vCPU held, and returns only a `FrameSinkFlow`
  (`crates/dh-vmm/src/runctl.rs:686-702`).
- The worker's per-frame hook (`crates/dh-worker/src/service.rs:3540-3579`) only
  reads the framebuffer region, lz4-compresses, and emits — it does not mutate
  the rail's device models, DHILOG, or chain.
- Live-injected inputs are applied through the same `apply_queued_input` rail
  path as pre-scheduled inputs and logged, and `observe_frame`'s `target <=
  frame` drain plus the leftover re-queue on `deactivate` mean an accepted input
  is landed-and-logged or carried forward, never silently dropped
  (`crates/dh-worker/src/runtime.rs:478-508`, `service.rs:3644-3663`).
- `run_segment_with_frame_captures` is driven on the `dh-slot-{id}` actor thread
  (the streaming handler's closure is dispatched via `with_runtime_mut` ->
  `SlotActor::with_runtime_mut`, `service.rs:5203`, `runtime.rs:240-259`), so the
  vCPU blocking hold is on the dedicated OS thread, not a shared async runtime.
- No crate below dh-worker gains a dh-worker dependency (verified
  `crates/dh-vmm`, `dh-detclock`, `dh-devices` Cargo manifests).

---

## Important

### I1. Terminal `Done` send can block the detached frames thread forever after the watchdog frees the vCPU

**Severity:** Important
**File:** `crates/dh-worker/src/service.rs:5248-5257` (terminal send), with the
watchdog at `5174-5181` and channel capacity at `1456`.

The watchdog's stated purpose is to stop "hold[ing] the vCPU, the slot's actor
thread, and its pinned core forever" for a consumer that keeps the connection
open but stops reading. It does free the vCPU: the hook returns
`FrameSinkFlow::Stop`, `drive_recorded_run` lands the slot `Paused`, and the
actor thread parks. But the orchestration thread (`dh-frames-{slot_id}`, a
detached `std::thread` with no retained join handle) then executes:

```rust
let _ = tx.blocking_send(Ok(proto::FrameCaptureEvent {
    msg: Some(proto::frame_capture_event::Msg::Done(done)),
}));
```

At the moment the watchdog fires, the bounded(2) channel is *full* (the two
buffered frames are exactly why `try_send` returned `Full`), and the current
frame was dropped without sending. A consumer that holds the HTTP/2 stream open
but never reads again leaves this `blocking_send` parked forever, leaking the
`dh-frames` thread plus ~2 buffered framebuffers. Because the slot is now
`Paused` and reusable, the *same* misbehaving client can start another
`RunWithFrameCapture` on the same lease, stall it past the watchdog again, and
repeat — accumulating stuck threads without bound from a single lease. This
partially defeats the watchdog's resource-protection intent (the vCPU/core is
freed, but threads and framebuffer memory are not) and there is no lease TTL to
reap it.

**Suggested fix:** bound the terminal `Done` send the same way the frame sends
are bounded — e.g. `try_send` in a short loop with the same watchdog deadline,
or `send_timeout`, and drop the `Done` (log it) if the consumer is gone:

```rust
// Terminal send must not outlive the watchdog: a stalled-but-open
// consumer must not pin this orchestration thread.
match result {
    Ok(done) => {
        let done_evt = Ok(proto::FrameCaptureEvent {
            msg: Some(proto::frame_capture_event::Msg::Done(done)),
        });
        let deadline = Instant::now() + FRAME_STREAM_STALL_WATCHDOG;
        let mut evt = done_evt;
        loop {
            match tx.try_send(evt) {
                Ok(()) => break,
                Err(TrySendError::Closed(_)) => break,
                Err(TrySendError::Full(returned)) if Instant::now() >= deadline => {
                    let _ = returned; // consumer abandoned the stream; drop Done
                    break;
                }
                Err(TrySendError::Full(returned)) => {
                    evt = returned;
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
    Err(e) => { let _ = tx.blocking_send(Err(e)); }
}
```

**Research reference:** `tokio-channel-streaming-deadlocks.md` — "Blocking
facades hide the runtime flavor from callers; a deadlock manifests as the
calling thread stuck ... the worst failure mode (hang, not error)." The
`blocking_send` here is on a raw OS thread rather than a runtime worker, so it
does not wedge the async runtime, but it is the same hang-not-error class the
watchdog was introduced to prevent.

---

### I2. The normative capture-neutrality invariant (proto C5) is only exercised by `#[ignore]`-gated M9 tests

**Severity:** Important (test coverage of a normative invariant)
**Files:** worker hook `crates/dh-worker/src/service.rs:3540-3579`; the only
default-runnable neutrality assertion is the vmm test
`crates/dh-vmm/src/runctl.rs:2118-2185`
(`frame_sink_observes_every_frame_and_is_capture_neutral_live`); the
worker-level check is
`crates/dh-worker/tests/frame_capture_stream.rs:147-231`
(`linux_streaming_capture_is_neutral_...`), which is `#[ignore]`d and needs the
staged `DH_M9_*` artifacts.

The proto amendment makes capture-neutrality normative ("the capture MUST NOT
perturb execution, the DHILOG, or the state hash ... CI-tested"). But the two
guards do not overlap on the risky part:

- The vmm test's sink is `|mark| { marks.push(mark); Ok(Continue) }` — it never
  reads the framebuffer. It proves the *sink mechanism* is neutral (plain vs.
  observed run land identically), not that the worker's real hook is.
- The worker's real hook calls `read_framebuffer_region_from_bus(&mut bus, ...)`
  every frame. If any device model state that feeds `hash_device_sections`
  (`bus` + `lapic`) were mutated by that read, the streamed run's epoch/final
  links would diverge from a plain Run. The only test that exercises a *real*
  framebuffer read against a plain-Run reference is the `#[ignore]`d M9 one, so
  a regression here passes default CI.

**Suggested fix:** add a host-runnable (or non-ignored, KVM-early-return) test
that drives `drive_recorded_run` with a real framebuffer-reading hook and
asserts terminal `state_hash` equality against a plain Run over the same budget
— i.e. lift the neutrality assertion of
`linux_streaming_capture_is_neutral_...` to a form that runs in the default lane
(even on the pad_echo fixture with a published framebuffer region), or wire the
M9 neutrality leg into a required CI job. This closes the gap between "the sink
plumbing is neutral" (already covered) and "the actual per-frame framebuffer
read is neutral" (currently only operator-run).
</content>
