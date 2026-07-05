# Resolution

## Outcome

The originally reported real-artifact failure does not reproduce on this
checkout. The local M9 gate reaches post-Ready frames on both the fresh boot and
restored arms.

No production `dh-worker`, `dh-vmm`, or restore-engine code was changed. The
implementation adds a public regression fixture and test that directly proves
the worker drains detchannel ring-W `FrameMark` events before pv-pad
`FRAME_COUNTER` writes satisfy `Run{frame_budget}`.

## Root cause in current tree

Current evidence does not show a live production-code defect. The earlier red
state was either fixed before this branch or depended on state outside this
checkout. The remaining gap was test coverage: CI had a fake-frame fixture that
wrote `FRAME_COUNTER` directly and therefore did not prove the detchannel
ring-W drain seam.

## Files changed

- `tests/nanokernel/asm/detchannel_frames.asm`
- `tests/nanokernel/build.rs`
- `tests/nanokernel/src/lib.rs`
- `tests/nanokernel/tests/channel_interop.rs`
- `tests/nanokernel/tests/elf_shape.rs`
- `crates/dh-worker/tests/m5_frame_scheduling.rs`

## New regression

`detchannel_frame_budget_drains_sdk_frame_marks_across_restore` drives:

1. `CreateVm`
2. `Run{frame_budget=3}`
3. `StreamGuestEvents(FrameMark)` and payload decode
4. `TakeSnapshot`
5. `RestoreSnapshot`
6. `Run{frame_budget=2}`
7. `StreamGuestEvents(FrameMark)` and payload decode
8. sealed DHILOG frame-table checks for absolute pv-pad frames `[1,2,3]` and
   `[4,5]`

Observed output:

```text
detchannel-frame-budget first_icount=388 sdk_frames=[(64, 1), (225, 2), (386, 3)] first_marks=[(66, 1), (227, 2), (388, 3)] restored_icount=710 restored_sdk_frames=[(547, 4), (708, 5)] restored_marks=[(161, 4), (322, 5)]
```

The SDK-event icounts precede the frame-counter icounts, proving the intended
publish/drain-before-frame-counter ordering.

## Verification

```bash
cargo test -p nanokernel
cargo test -p dh-worker --test m5_frame_scheduling \
  detchannel_frame_budget_drains_sdk_frame_marks_across_restore -- --nocapture
cargo test -p dh-worker --test m5_frame_scheduling -- --nocapture
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
  DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage \
  DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio \
  DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
  DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
  DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache \
  cargo test -p dh-worker --test m5_frame_scheduling \
    linux_m5_frame_budget_records_post_ready_frame_marks -- --ignored --nocapture
```

All passed locally.
