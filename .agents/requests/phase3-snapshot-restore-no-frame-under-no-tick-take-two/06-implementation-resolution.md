# Implementation resolution

## What changed

- Replaced the detchannel frame fixture's explicit W-doorbell with the
  SDK-normal path: publish ring-W `FrameMark`, then write pv-pad
  `FRAME_COUNTER`.
- Drained detchannel at successful 4-byte pv-pad `FRAME_COUNTER` MMIO writes
  in the worker service path, after the pv-pad write logs the DHILOG
  `FRAME_MARK`.
- Mirrored the same frame-boundary drain in replay.
- Fixed replay final-link handling for logs that reach `END.end_icount` by
  replaying generated frame-boundary records and therefore have no tail run to
  hash.
- Added real-emulator initramfs provenance checks that require the real
  `/usr/bin/refwork-harness` boot contract and reject stale synthetic
  `/opt/m9-refwork-contract` artifacts.
- Added real-game and NOP-game diagnostics that print artifact provenance,
  frame counters, and buffered SDK events on frame-budget hard-cap failures.

## Verification

Passed:

```bash
cargo test -p dh-worker --test m5_frame_scheduling \
  detchannel_frame_budget_drains_sdk_frame_marks_across_restore -- --nocapture

cargo test -p dh-worker --test m5_frame_scheduling -- --nocapture

cargo test -p nanokernel

cargo test -p dh-worker
```

The focused no-doorbell worker test now reports:

```text
first_icount=382 sdk_frames=[(64, 1), (223, 2), (382, 3)] first_marks=[(64, 1), (223, 2), (382, 3)]
restored_icount=700 restored_sdk_frames=[(541, 4), (700, 5)] restored_marks=[(159, 4), (318, 5)]
```

The restored SDK icounts are cumulative worker icounts; the restored DHILOG
marks are segment-relative and match after adding the first segment's
`382`-instruction base.

## Remaining red evidence

With the real reference-workload artifacts at commit
`7e94a828b2b9d252cff511cef5fc8baa4836caca`, both ignored M9 diagnostics still
hard-cap after Ready:

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
DH_M9_BZIMAGE=/home/infra-admin/git/preestablished/reference-workload/dist/workload-image-0.1.0/bzImage \
DH_M9_INITRAMFS=/tmp/dh-real-m9.DlWKwn/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/tmp/dh-real-m9.DlWKwn/image-cache \
cargo test -p dh-worker --test m5_frame_scheduling \
  linux_m5_frame_budget_records_post_ready_frame_marks -- --ignored --nocapture
```

Result: `HardCap`, ready frame counter `0`, post-run frame counter `0`, and no
post-ready `FrameMark` events in the buffered SDK event stream.

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
DH_M9_BZIMAGE=/home/infra-admin/git/preestablished/reference-workload/dist/workload-image-0.1.0/bzImage \
DH_M9_INITRAMFS=/tmp/dh-real-m9.DlWKwn/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/tmp/dh-real-m9.DlWKwn/image-cache \
cargo test -p dh-worker --test m5_frame_scheduling \
  linux_m5_real_emulator_nop_game_frame_budget_diagnostic -- --ignored --nocapture
```

Result: the NOP override hash
`0c26841f15654a4dc38d31b4b41c231ef5b6eeda8fceb366c160a5517265a82e` also
hard-caps with ready frame counter `0` and post-run frame counter `0`.

Conclusion: the determinism-hypervisor worker no-doorbell drain gap is fixed
and replay-covered. The real M9 Linux workload still does not emit post-ready
frame marks under this no-tick run; that remains a separate guest/workload
follow-up and is not claimed fixed here.
