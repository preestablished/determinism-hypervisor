# Positive Notes — 2nd reviewer

### P1. The extract-to-`drive_recorded_run` refactor is faithful, not just a move

`crates/dh-worker/src/service.rs:3232-3630`. The ~370-line `Run` actor body was
lifted verbatim into `drive_recorded_run` and re-parameterized on `frame_hook`
and `live_inputs`. For plain `Run` both are `None`, so `frame_sink` returns
`Continue` and `live_input_sink` returns `Ok` immediately — the epoch
checkpointing, SDK-event stop, pause roll-forward, and boundary bookkeeping are
byte-for-byte the same code path. This is the low-risk way to add a streaming
sibling without forking behavior, and the shared `play_perf_smoke` relative
regression guard (streamed fps must not fall below per-frame-Run fps in the same
process) directly guards against a per-frame chain link creeping back in.

### P2. Backpressure is implemented as "hold the vCPU," not "drop frames or buffer unboundedly"

`crates/dh-worker/src/service.rs:1355-1390`. The `try_send` + 1ms-sleep loop on a
capacity-2 channel means a stalled consumer literally pauses the guest at the
FRAME_MARK boundary — no frame is ever dropped, and the vCPU stays at a
deterministic icount. This is exactly right for a determinism engine, and the
neutrality/backpressure M9 test asserts a slow consumer lands bit-identically to
a fast one. It also correctly uses a dedicated `std::thread` rather than
`spawn_blocking`, avoiding oversubscription of the tokio blocking pool and making
the terminal `blocking_send` legal (per the spawn-blocking research note).

### P3. Live-inject validation and drain are serialized under one mutex — no TOCTOU

`crates/dh-worker/src/runtime.rs:471-519` + `service.rs:216-264`. Both
`live_inject_from_proto` (validate against `last_streamed_frame` + append) and
`observe_frame` (advance `last_streamed_frame` + drain) run under the same
`SlotLiveInputs` mutex. An input is either accepted for a strictly-future frame
and later drained, or rejected — there is no window where it is accepted yet
provably unreachable, and unreached-but-accepted inputs are re-queued on
`deactivate` (service.rs:786-802). The order-space split (`1 << 63` for live vs
0-based for the runtime, re-based on re-queue) cleanly avoids order collisions.

### P4. FrameSink stop precedence and the frame-mark GPA are both pinned by tests

`crates/dh-vmm/src/runctl.rs:722-765` gives one unwind policy (HLT >
frame-sink-stop > event-stop > real error) so the flags — not the sentinel error
— decide the outcome, and `frame_sink_stop_lands_paused_at_the_frame_boundary_live`
proves a sink `Stop` at frame 3 lands identically to a `FrameBudget{frames:3}`
stop (same boundary, same state hash). `frame_mark_gpa_is_pinned_to_the_device_window`
(runctl.rs:1922) is a host-runnable guard that a silent device-constant move
cannot pass the live tests while breaking real rails — good defensive testing.

### P5. Metrics label set is closed and self-checking

`crates/dh-worker/src/service.rs:21-30, 61-68, 127-132`. The termination reasons
are a fixed `FRAME_STREAM_TERMINATION_LABELS` slice; `render` iterates that list
(so an unrecorded label is impossible to emit silently), and
`record_frame_stream_termination` `debug_assert!`s membership. Every reachable
`StopReason` maps to a label, with `"other"` as the safety net for the
frame-capture-impossible reasons. The `build_profile` field addition to
`GetWorkerInfo` is a pure additive proto change (field 6, no renumber), matching
the proto-evolution rules.

### P6. Proto change is additive and correctly scoped

`proto/hypervisor.proto:41,373-405,416`. `RunWithFrameCapture` and its messages
were already declared; this fills in `build_profile = 6` on `GetWorkerInfoResponse`
(new tag, no reuse) and the capture-neutrality contract is written normatively
into the schema comment. The `RunWithFrameCaptureRequest.until` deliberately
carries only the budget arms, with cancellation as the "run until I say stop"
mechanism — a clean, well-documented decision.
