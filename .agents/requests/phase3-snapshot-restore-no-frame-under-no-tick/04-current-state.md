# Current State

## Branch and tracking

- Work branch: `codex/determinism-hypervisor-tqvb-phase3-no-frame-restore`
- Local bead: `determinism-hypervisor-tqvb`
- Request source: this directory, especially `03-ask.md`

## Request contract

The request asks determinism-hypervisor to address a no-tick post-Ready
frame-budget failure observed through `dh-workerd`:

- `Run{frame_budget=N}` must stop with `BUDGET_REACHED`, not `HARD_CAP`.
- `frames_elapsed` must equal `N`.
- The path must work on a freshly booted Ready workload and after snapshot
  restore.
- The regression must cover the real frame path: guest `FrameMark` on ring W
  followed by pv-pad `FRAME_COUNTER`, not a fake guest that writes
  `FRAME_COUNTER` directly.
- Determinism behavior, state hashes, DHILOG contents, and replay paths must not
  be perturbed.

## Local verification before implementation

On 2026-07-05, the documented red repro was rerun locally on the current tree
with the staged M9 artifacts:

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

Result: pass.

Important output:

```text
linux-m5 frames start=0 first_icount=641818110 first_frames=[(186992, 1), (330795, 2), (474598, 3)] restored_icount=642105716 restored_frames=[(143803, 4), (287606, 5)]
test linux_m5_frame_budget_records_post_ready_frame_marks ... ok
```

This contradicts the originally reported red state for the local checkout. It
proves the local checkout can reach pv-pad `FRAME_COUNTER`, stop
`Run{frame_budget}` with `BUDGET_REACHED`, and preserve absolute frame-counter
continuity across restore for the staged real M9 workload on this host.

It does not, by itself, prove that dh-worker drained detchannel `FrameMark`
SDK events. The current test's `frame_marks()` helper reads DHILOG
`RecordBody::FrameMark`, which is produced by pv-pad `FRAME_COUNTER` writes.
Direct detchannel proof requires `SDK_EVENT` records or
`StreamGuestEvents(FrameMark)` payloads.

## Consequence

The implementation should not make speculative production changes to
`dh-worker`, `dh-vmm`, or detchannel restore logic unless a current failing
test exposes a concrete defect. The current missing deliverable is a public,
non-private-artifact regression gate that proves both halves of the intended
path:

- detchannel ring-W `FrameMark` events are drained by dh-worker; and
- the following pv-pad `FRAME_COUNTER` writes satisfy `Run{frame_budget}` before
  and after restore.

Also add a resolution note that records the H1/H2 outcome and current
verification.
