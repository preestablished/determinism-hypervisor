# Corrected hypothesis

## What is proven

- The first reported worker repro is real with the real-emulator initramfs.
- The first repro fails before restore.
- The stale-cache pass used a synthetic contract fixture and did not prove the
  deployed workload.
- The old fixture's explicit per-frame doorbell was not faithful to
  guest-sdk's normal `frame_mark()` path.
- A guest-sdk probe with the real emulator and real game reached `Ready` but
  did not emit a frame within 30 seconds, so the game/content path is a live
  variable.

## Primary working theory

There are two separate issues that the first plan conflated:

1. **Worker semantic gap:** dh-worker should drain detchannel at the
   `FRAME_COUNTER` MMIO exit, because SDK `FrameMark` records normally do not
   doorbell when ring W has room. This matters for event ordering, DHILOG
   `SDK_EVENT` icounts, and `StreamGuestEvents` fidelity.
2. **Real-game no-frame condition:** the real emulator plus real game does not
   reach the first `frame_mark()` quickly in either the worker repro or the
   controlled guest-sdk probe. This may be an emulator/game/content issue, an
   instruction-budget issue, or a still-unisolated host difference.

## What not to assume

Do not assume a worker-side ring-drain fix will make the real game reach its
first frame. It can only drain a record after the guest publishes one or reaches
the following `FRAME_COUNTER` write.

Do not use the synthetic `/opt/m9-refwork-contract` initramfs as proof for the
deployed real-emulator workload.

Do not use guest-sdk + NOP ROM as proof for dh-worker + real game. That changes
two variables at once.
