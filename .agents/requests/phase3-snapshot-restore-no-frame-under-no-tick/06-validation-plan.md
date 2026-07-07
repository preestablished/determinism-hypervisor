# Validation Plan

## Required local gates

Run the new public fixture test:

```bash
cargo test -p dh-worker --test m5_frame_scheduling \
  detchannel_frame_budget_drains_sdk_frame_marks_across_restore -- --nocapture
```

Run the nanokernel fixture checks:

```bash
cargo test -p nanokernel
```

Run the real-artifact M9 acceptance gate when artifacts and KVM are available:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_GUEST=linux \
DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage \
DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache \
cargo test -p dh-worker --test m5_frame_scheduling \
  linux_m5_frame_budget_records_post_ready_frame_marks -- --ignored --nocapture
```

Run focused m5 coverage:

```bash
cargo test -p dh-worker --test m5_frame_scheduling
```

## Acceptance evidence

The implementation is complete only when evidence proves:

- the public fixture stops `BUDGET_REACHED` for both fresh boot and restored
  runs;
- `frames_elapsed` equals the requested frame budget;
- the input log contains strict absolute frame marks before and after restore;
- the test proves drained detchannel `FrameMark` events directly, not only
  pv-pad frame-counter writes;
- the real M9 ignored gate still passes locally when artifacts are present;
- no production run-control behavior changed without a failing test requiring
  it.

## Session close

Per this repo's bead workflow:

1. update or close `determinism-hypervisor-tqvb`;
2. commit the request docs and code changes;
3. run `bd dolt push`;
4. push the git branch.
