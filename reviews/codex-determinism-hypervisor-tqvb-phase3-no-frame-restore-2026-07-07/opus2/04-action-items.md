# Action Items — 2nd reviewer

## Action Items

### Critical
- [ ] None.

### Important
- [ ] [crates/dh-worker/src/service.rs:1446-1453] Bound the terminal
      `blocking_send(Ok(Done))` / `blocking_send(Err(e))` so a never-reading
      consumer cannot pin the `dh-frames-N` thread after the watchdog frees the
      vCPU/actor/core. Use a timed send (block_on `timeout(watchdog, tx.send)`)
      or a `try_send` retry loop bounded by `FRAME_STREAM_STALL_WATCHDOG`. (I1)
- [ ] [crates/dh-worker/src/replay_engine.rs:2071-2081] Verify that dropping the
      `terminal_sdk_target.is_none()` guard and switching the segment-final-link
      condition to a live counter read (`counter.read() == header.end_icount`)
      does not add or shift a hash link for `next_sdk_event`-terminated replays;
      add a targeted replay test or justify inertness in the commit. (I2)
- [ ] [crates/dh-worker/src/service.rs:47-59, 1366-1388] Make
      `frame_holds_in_progress` unwind-safe (RAII decrement guard) so an
      actor-thread panic mid-hold does not strand the gauge at a nonzero value.
      Low priority within Important. (I3)

### Suggestions
- [ ] [crates/dh-worker/src/service.rs:4498-4509] Close or document the
      start-of-run window where `InjectInputs` sees `with_active == None` and then
      queues behind a just-started streaming run on the actor channel. (S1)
- [ ] [crates/dh-worker/src/runtime.rs:444-508] Rename `last_streamed_frame` to
      `last_reached_frame` (or similar) to reflect that it is the run's internal
      position, not the consumer's — the acceptance floor must stay the run
      position for determinism. (S2)
- [ ] [crates/dh-worker/src/service.rs:786-802] Note that live leftovers
      re-queued as static `Frame(target)` inputs lose the `<=` catch-up
      semantics (exact-match only), so a skipped frame could drop them across the
      run boundary. (S3)
- [ ] [crates/dh-worker/src/service.rs:88-96] Fix the
      `frame_emit_duration_milliseconds` help text or move `emit_start` before
      the fb read/lz4 so measurement matches the description. (S4)
- [ ] [crates/dh-worker/tests/play_perf_smoke.rs:31,
      frame_capture_stream.rs:24,27,391] Derive M9 budgets from a single named
      instr-per-frame estimate with an explicit headroom multiplier; assert
      `BUDGET_REACHED` per per-frame `Run` in Phase A. (S5)
- [ ] [crates/dh-worker/tests/frame_capture_stream.rs] Add a host-level
      (non-KVM) test of the hook state machine (full→hold→Closed cancel;
      watchdog deadline; terminal delivery with a stalled sink) — this is where
      I1 would surface off-M9. (S6)
