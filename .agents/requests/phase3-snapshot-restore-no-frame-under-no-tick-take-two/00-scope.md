# Phase 3 no-frame request, take two

## Why take two exists

The first resolution was useful but did not close the reported bug. It proved
that dh-worker can drain a synthetic detchannel frame fixture, but the local M9
pass used a stale synthetic initramfs:

- synthetic cache payload: `/opt/m9-refwork-contract`
- deployed/request payload: `/usr/bin/refwork-harness`

The real emulator initramfs still reproduces the fresh-boot `Run{frame_budget}`
hard-cap on this checkout.

## Corrected scope

This is no longer a restore-first investigation. The failure happens before the
snapshot/restore arm:

1. boot real emulator initramfs to `Ready`;
2. run the same live VM with `frame_budget = 1`;
3. stop reason is `HARD_CAP`, not `BUDGET_REACHED`.

The implementation should close the worker-side evidence gaps and avoid claiming
a dh-worker production fix unless a current failing test proves that layer.

## Current target

Make the repo distinguish these cases explicitly:

- real emulator initramfs vs synthetic contract initramfs;
- real game image vs NOP/minimal game image;
- detchannel `FrameMark` with explicit doorbell vs the SDK's normal
  no-doorbell-unless-full `frame_mark()` path;
- fresh boot vs restore.

The final answer for this repo may be either a dh-worker fix or a proven
handoff to reference-workload/guest-sdk, but it must be based on controlled
evidence rather than the previous stale-artifact pass.
