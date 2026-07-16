# Action Items

### Critical
- [ ] None.

### Important
- [ ] [crates/dh-worker/src/service.rs:5248-5257] Bound the terminal `Done`
      send so a stalled-but-open consumer cannot pin the detached
      `dh-frames-{slot}` thread forever after the watchdog frees the vCPU. Use
      `try_send` with the `FRAME_STREAM_STALL_WATCHDOG` deadline (or
      `send_timeout`) and drop `Done` if the consumer is gone. Prevents an
      unbounded per-lease thread + framebuffer-memory leak from a repeat-stall
      client. (See I1.)
- [ ] [crates/dh-worker/tests/frame_capture_stream.rs:147-231 /
      crates/dh-worker/src/service.rs:3540-3579] Add a default-lane (non-M9,
      KVM-early-return) test that drives `drive_recorded_run` with a *real*
      framebuffer-reading hook and asserts terminal `state_hash` equality vs a
      plain Run over the same budget, or wire the M9 neutrality leg into a
      required CI job. The normative C5 capture-neutrality invariant is
      currently only guarded by `#[ignore]`d M9 tests; the non-ignored vmm test
      uses a no-op sink that never reads the framebuffer. (See I2.)

### Suggestions
- [ ] [crates/dh-worker/src/service.rs:362-369] Make
      `record_frame_stream_termination` release-safe: fall back to `"other"` for
      an unlisted label instead of only `debug_assert!`-ing, so a future
      mislabel is not silently dropped by `render`. (S1)
- [ ] [crates/dh-worker/src/service.rs:5159-5194] Add a comment marking the
      `try_send`/`sleep(1ms)` backpressure loop as intentionally non-`await` so
      it is not "optimized" into `.send().await` (which would move the block onto
      a runtime worker). (S2)
- [ ] [crates/dh-worker/src/service.rs:1426-1449] Either document in the proto
      that `hard_icount_cap` is ignored by `RunWithFrameCapture`, or reject a
      nonzero value with `InvalidArgument`. (S3)
- [ ] [crates/dh-worker/src/service.rs:3242-3745] Consider extracting
      `epoch_sink`'s ~120-line bisection-checkpoint body into a named helper to
      reduce nesting and the `too_many_arguments` surface. (S4)
- [ ] [crates/dh-worker/src/service.rs:5133-5258] Add a
      frame-orchestration-thread gauge (inc on spawn / dec at end) to surface any
      stuck detached thread in metrics. (S5)
- [ ] [crates/dh-worker/src/replay_engine.rs:2071-2078] Add a one-line comment
      explaining why the physical `counter.read()` (not the last canonical
      record icount) is authoritative for the terminal device-link condition, so
      the no-doorbell-frame-boundary fix is not reverted later. (S6)
</content>
