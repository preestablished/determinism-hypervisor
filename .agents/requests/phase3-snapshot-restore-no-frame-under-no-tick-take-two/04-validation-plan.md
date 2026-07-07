# Validation plan

## Required local gates

Run the public nanokernel coverage:

```bash
cargo test -p nanokernel
```

Run the focused worker detchannel frame-budget gate:

```bash
cargo test -p dh-worker --test m5_frame_scheduling \
  detchannel_frame_budget_drains_sdk_frame_marks_across_restore -- --nocapture
```

That gate must prove the SDK-normal no-doorbell path. `StreamGuestEvents`
`FrameMark` icounts should be strict and should match the DHILOG
`FRAME_MARK` boundary after converting the DHILOG segment-relative icount to
the worker API's cumulative icount domain.

Run replay validation over the sealed no-doorbell detchannel log:

```bash
cargo test -p dh-worker --test m5_frame_scheduling \
  detchannel_frame_budget_drains_sdk_frame_marks_across_restore -- --nocapture
```

The same test should call `VerifyReplay`; a separate command is not needed if
the assertion is in the test body.

Run the full non-ignored worker frame scheduling target:

```bash
cargo test -p dh-worker --test m5_frame_scheduling -- --nocapture
```

Run the real-emulator worker repro with the real game and capture the expected
current red evidence if the game/content issue remains:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_GUEST=linux \
DH_M9_BZIMAGE=/home/infra-admin/git/preestablished/reference-workload/dist/workload-image-0.1.0/bzImage \
DH_M9_INITRAMFS=/tmp/dh-real-m9.DlWKwn/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/tmp/dh-real-m9.DlWKwn/image-cache \
cargo test -p dh-worker --test m5_frame_scheduling \
  linux_m5_frame_budget_records_post_ready_frame_marks -- --ignored --nocapture
```

Run the new controlled real-emulator + NOP-game worker diagnostic.

## Acceptance evidence

The repo-local implementation is complete when:

- detchannel frame coverage proves no-doorbell `FrameMark` drain at
  `FRAME_COUNTER`;
- replay verifies the same drain semantics from a sealed no-doorbell log;
- real-emulator worker gates reject the stale synthetic contract when real
  emulator evidence is required;
- the real-game red repro reports enough evidence to hand off ownership if it
  remains red;
- the controlled NOP-game run establishes whether changing only the game image
  changes worker frame progress.

If the real game remains no-frame in both dh-worker and guest-sdk, do not claim
the deployed workload is fixed in determinism-hypervisor. Record the evidence
and file follow-up against the owning repo/workstream.
