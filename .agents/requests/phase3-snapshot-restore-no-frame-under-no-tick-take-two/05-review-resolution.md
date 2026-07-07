# Review resolution

Two subagents reviewed the take-two plan before implementation.

## Accepted feedback

- The worker/replay `FRAME_COUNTER` drain is coherent with `runctl`, because
  `on_exit` is serviced before frame-budget counting and stop handling.
- The drain must happen only for a successful 4-byte pv-pad `FRAME_COUNTER`
  write, not for arbitrary MMIO writes.
- Service and replay must use the same ordering: pv-pad write first, then
  detchannel drain at the same boundary.
- The fixture must cover SDK-normal no-doorbell `FrameMark` publication, not
  only explicit W-doorbell publication.
- The test should verify replay from the sealed no-doorbell log.
- The NOP-game diagnostic should be phrased as implicating the game/content
  path, not proving the real game is solely at fault.
- Real-emulator artifact checks should be exact enough to reject stale
  synthetic and hybrid initramfs artifacts.

## Implementation adjustments

The implementation will:

- convert the frame fixture to SDK-normal no-doorbell frame marks;
- drain detchannel on frame-counter MMIO in both `service.rs` and
  `replay_engine.rs`;
- add replay verification to the service-level detchannel test;
- add worker-side real-emulator provenance helpers and diagnostics;
- keep the real-game no-frame outcome separate from repo-local worker fixes.
