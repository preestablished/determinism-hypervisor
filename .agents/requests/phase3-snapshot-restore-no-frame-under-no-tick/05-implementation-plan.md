# Implementation Plan

## 1. Add a public frame-marking fixture

Create a nanokernel guest that exercises the real host-side path under test:

- initialize the detchannel page with the same clean-room layout used by
  `device_exercise.asm`;
- emit one ring-W `FrameMark` record per frame:
  `len=24`, `kind=13`, `seq=frame-1`, `vnanos` sampled from pv-clock or zero,
  payload `frame_index`;
- maintain a monotonic ring-W producer byte index; do not rewrite offset zero
  or republish `prod=24` after the first event;
- write a `Pad` record before wrapping when the tail cannot hold a 24-byte
  `FrameMark`;
- publish the ring-W producer index before any drain;
- ring `PORT_DOORBELL` with `DOORBELL_RING_W` for this fixture's explicit
  doorbell-drain coverage;
- after the doorbell exit is serviced, write the same frame index to pv-pad
  `FRAME_COUNTER`;
- loop forever with a fixed busy cadence.

This fixture should be separate from `fake_frames.asm`, because `fake_frames`
writes `FRAME_COUNTER` directly and does not exercise ring-W drain/doorbell
servicing. It is a worker doorbell-drain regression fixture, not a perfect
copy of guest-sdk `frame_mark()`: current guest-sdk publishes the W event,
uses the critical doorbell callback on ring-full retry, and then writes
`FRAME_COUNTER`. The test value here is that dh-worker must service the
doorbell-drain seam before the frame counter can advance.

Expected files:

- `tests/nanokernel/asm/detchannel_frames.asm`
- `tests/nanokernel/build.rs`
- `tests/nanokernel/src/lib.rs`
- `tests/nanokernel/tests/elf_shape.rs`
- `tests/nanokernel/tests/channel_interop.rs`

## 2. Add a worker API regression test

Add a non-ignored `dh-worker` test that boots the new fixture through
`WorkerService`, then drives:

1. `CreateVm`
2. `Run{frame_budget=3}`
3. assert `BUDGET_REACHED` and `frames_elapsed == 3`
4. `TakeSnapshot`
5. `RestoreSnapshot`
6. `Run{frame_budget=2}`
7. assert `BUDGET_REACHED` and `frames_elapsed == 2`
8. inspect sealed input logs and assert absolute pv-pad frame marks
   `[1, 2, 3]` then `[4, 5]`
9. assert detchannel `FrameMark` events were drained, either by
   `StreamGuestEvents` or by sealed DHILOG `SDK_EVENT` records whose stream is
   `EventKind::FrameMark`

The test must configure at least two worker slots, because restore allocates a
second lease while the original VM is still alive. It must also use at least
8 MiB RAM so the fixture's channel page at `0x400000` plus 2 MiB fits.

This keeps coverage on the service-level code path that owns:

- `service_exit_with_detchannel`
- runtime drained-event buffering
- `runctl::Until::FrameBudget`
- snapshot/restore device state, including pv-pad and detchannel EVTC

Expected file:

- `crates/dh-worker/tests/m5_frame_scheduling.rs`

## 3. Leave production code unchanged unless the new gate fails

If the new public gate passes and the real M9 artifact gate remains green,
do not edit production run-control or restore code. The current code already
proves:

- pv-pad `FRAME_COUNTER` writes are reached on the worker path;
- `FrameBudget` stops at the frame-boundary exit;
- restore preserves the absolute pv-pad counter.

The new public gate must add the missing direct proof that ring-W `FrameMark`
SDK events are drained on the worker path.

If the new gate fails, fix the narrow failing layer shown by that failure before
changing broader runtime behavior.

## 4. Record resolution

Add a resolution markdown file in this request directory with:

- current H1/H2 status;
- local command evidence;
- root cause as determined by current evidence;
- files changed;
- final verification commands.

The resolution should explicitly say whether the original report still
reproduces on this checkout. Based on the first local run, it does not.
