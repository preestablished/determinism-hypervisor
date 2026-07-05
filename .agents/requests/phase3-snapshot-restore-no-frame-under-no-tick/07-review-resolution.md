# Review Resolution

Two subagents reviewed `04-current-state.md`, `05-implementation-plan.md`, and
`06-validation-plan.md`.

## Accepted review points

- The first local M9 pass proves `FRAME_COUNTER`, `FrameBudget`, and restore
  continuity, but not detchannel `FrameMark` drain. The plan now states that
  distinction.
- The public regression must assert detchannel `FrameMark` events directly via
  `StreamGuestEvents` or DHILOG `SDK_EVENT`, in addition to pv-pad
  `FRAME_MARK` records.
- The worker test needs at least two slots because restore creates a second
  lease while the original slot is still occupied.
- The fixture must keep a monotonic ring-W producer byte index and handle
  record wrapping with `Pad`, instead of reusing the one-shot
  `device_exercise.asm` pattern.
- The fixture should have nanokernel drift/interoperability checks because it
  hard-codes detchannel ABI values.

## Noted nuance

The fixture is explicit doorbell-drain coverage. It is not a byte-for-byte
guest-sdk `frame_mark()` clone: current guest-sdk uses the critical doorbell
callback when ring W is full, then writes `FRAME_COUNTER`. The public fixture is
still useful because the original report suspected dh-worker's host-side
doorbell/drain seam, and it forces that seam before every frame counter write.

## Resulting implementation direction

Implement the public fixture and service-level regression test. Do not change
production run-control or restore code unless the new regression exposes a
current failing behavior.
