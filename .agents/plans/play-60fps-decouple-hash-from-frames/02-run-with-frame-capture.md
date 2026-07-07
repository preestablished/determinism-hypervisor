# M2 — Implement `RunWithFrameCapture`

## What exists already

- Proto surface is final and generated: `proto/hypervisor.proto` —
  `rpc RunWithFrameCapture (RunWithFrameCaptureRequest) returns (stream
  FrameCaptureEvent)`; `CapturedFrame{frame_index, icount, fb_lz4,
  fb_info}`; terminal `RunResponse`. Request supports
  `icount_budget | vns_budget` + `hard_icount_cap`.
- Normative spec text in the proto comments and API.md (the
  RunWithFrameCapture block sits after §2.7 Verification, before §2.8 —
  no numbered subsection; consider a docs bead to give it one):
  - one `CapturedFrame` per FRAME_MARK observed;
  - **capture-neutral**: a capture run and a plain run over the same
    (snapshot, inputs) produce identical refs and epoch hashes
    (CI-tested);
  - **backpressure**: if the consumer stalls, the worker holds the vCPU
    paused at the FRAME_MARK boundary; frames are never dropped.
- The worker stub returns `unimplemented`
  (`crates/dh-worker/src/service.rs:4709`).
- All the building blocks are live in the per-frame path today:
  FRAME_MARK exits are counted in every until-mode
  (`SegmentOutcome::frames_elapsed`, `crates/dh-vmm/src/runctl.rs`), and
  framebuffer extraction at a paused boundary exists
  (`framebuffer_capture` / `capture_at_boundary`,
  `crates/dh-worker/src/service.rs:2808/2828`, already lz4 + `FbInfo`).

## Why this is the 60fps enabler

With one long Run instead of one Run per frame:

- `hash_final_stop` links happen once per run stop instead of 60x/second;
- epoch links stay on the 50M-instruction grid (their defined cadence);
- per-frame cost becomes: guest execution + framebuffer copy + lz4 +
  stream send — all well under 16.6ms;
- the stream's backpressure hold gives the client exact real-time pacing
  (the bridge reads one frame per 1/60s tick; the vCPU waits at the
  boundary in between — no free-running, no frame drops).

## Implementation sketch

1. **vmm layer**: extend the segment driver so a caller can supply a
   per-frame callback at the FRAME_MARK exit (the pv-pad `0x1C`
   FRAME_COUNTER MMIO write, ARCH §6.4; the FRAME_MARK consistency rule
   is cross-referenced into §6.6) — either a new
   `run_segment_with_frame_captures(...)` variant beside the existing
   `run_segment_with_scheduled_inputs_frames_and_epochs`, or a
   `frame_sink: Option<&mut dyn FnMut(FrameMark) -> Result<...>>`
   parameter threaded through `run_segment_inner`. The callback runs with
   the vCPU paused ON the frame-boundary exit; blocking in it implements
   the backpressure hold. It must be read-only with respect to guest
   state (capture-neutrality).
2. **worker layer** (`run_with_frame_capture` in
   `crates/dh-worker/src/service.rs`): validate lease → occupy the slot
   runtime thread exactly like `run` does → drive the vmm variant. In the
   frame callback: read the framebuffer region via the existing
   `framebuffer_capture` path, lz4-compress, and `blocking_send` the
   `CapturedFrame` into the response stream's bounded channel (capacity
   1–2; a full channel is the backpressure hold). Blocking here is safe:
   the run occupies the slot's dedicated `SlotActor` OS thread
   (`crates/dh-worker/src/runtime.rs`), not a shared async runtime. On
   stream cancel (client dropped), stop the run at the current frame
   boundary and leave the slot Paused at a deterministic icount —
   document that landing in API terms (it is a Pause-equivalent stop;
   DHILOG must reflect a normal segment stop).
   **Stream-cancel is also the interactive-Stop path**: `PauseRequest`
   deliberately lands on the NEXT EPOCH boundary (API.md §2.4 /
   `runctl.rs` pause handling), which at ~1M instr/frame can be ~50
   frames (~1s) of extra play after the operator hits Stop. The bridge
   must stop a streaming run by cancelling the stream (frame-boundary
   landing, ≤1 frame), not by issuing Pause; state this in the API.md
   amendment so both latencies are documented and only one path is used.
2a. **Stalled-consumer watchdog**: cancel handles a *disconnected*
   client, but a wedged bridge that keeps the HTTP/2 connection open
   while never reading (GC pause, deadlock, half-open network) would
   hold the vCPU, the slot's actor thread, its pinned core, and a
   blocking-pool task forever — leases have no TTL
   (`LeasePolicy::default()`), so nothing reclaims the slot. Add a
   server-side hold watchdog: if the consumer has not advanced the
   stream within N seconds (worker constant, e.g. 30s), end the run at
   the held frame boundary exactly as a cancel would, and surface a
   distinct stop reason in the terminal event/log.
3. **Chain semantics**: the run pushes epoch links exactly as `run` does
   (same `epoch_sink`), and one `hash_final_stop` link at the terminal
   stop. No link per frame. No new hash inputs.
4. **Pad input during the run** is M3 (03); M2 alone is sufficient for
   the replay-renderer use case and for bridge play with segment-boundary
   input (bridge plan B2 fallback).

## Decisions to make during implementation

- Whether to also add a `frame_budget` arm to
  `RunWithFrameCaptureRequest.until`. The spec'd request only has
  icount/vns budgets; the bridge wants "run until I say stop". Options:
  (a) client sends a large `icount_budget` and cancels / Pauses to stop —
  no proto change; (b) API.md + proto amendment adding `frame_budget`.
  Start with (a); only amend the API if operating experience demands it.
- Stream channel capacity: 1 gives strictest pacing (vCPU always ≤1 frame
  ahead of the viewer); 2 tolerates client jitter. Make it a worker
  constant, not config, unless measurement says otherwise.

## Observability

The existing `run()` path records exit metrics (`metrics.record_exit`;
`/metrics` per API.md §2.8). Give the streaming path equivalents from day
one — this is how the 60fps outcome is observed in production beyond the
one-off perf smoke:

- frames emitted (counter) and frame-emit latency (histogram);
- backpressure-hold duration per frame (histogram) and holds-in-progress
  (gauge);
- stream terminations by reason (budget / cancel / watchdog / fault).

## Tests

- **Capture-neutrality (normative, CI)**: same READY snapshot + same
  scheduled inputs, run once via `run` (icount budget) and once via
  `run_with_frame_capture` to the same boundary → identical terminal
  `state_hash`, identical epoch hash sequence, identical DHILOG.
- **Frame completeness**: frame stream indices are exactly the
  FRAME_COUNTER values `first..=last` with no gaps/dups;
  `frames_elapsed` in the terminal `RunResponse` matches the count.
- **Backpressure**: a consumer that sleeps 100ms between reads produces
  identical results to a fast consumer (determinism unaffected by
  consumer timing); worker memory stays bounded.
- **Cancel mid-run**: client drops the stream; slot ends Paused at a
  frame boundary; a subsequent `run` from that boundary is deterministic
  vs an uninterrupted reference run.
- **Stalled consumer (distinct from cancel)**: client keeps the
  connection open but stops reading; the watchdog ends the run at the
  held boundary within its deadline; slot is Paused and reusable;
  determinism of a subsequent run is unaffected.
- **Nanokernel-level**: extend `tests/nanokernel` FRAME_MARK fixtures to
  cover the callback variant if the vmm API grows one.

## Perf acceptance for M2

Using the M0 harness adapted to the streaming RPC, on release builds:
sustained ≥60 captured frames/second on the M9 reference workload with
epoch links on (`EpochsOn`) — or, if epoch cadence makes 60fps
unreachable (see M0 instructions-per-frame datum), the measured gap is
documented and M4 in 03 is activated.
