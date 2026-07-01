# Related Context: Slot Exhaustion We Hit On The Way

Not part of the framebuffer ask, but you will see its fingerprints in the
worker's history and it may inform worker-side hardening.

## What Happened

The bridge holds VM leases in memory only. Each operator session start does
`RestoreSnapshot` (new slot + lease); stop does `DestroyVm`. When the bridge
service restarts with a session active — which happened several times on
2026-07-01 during deploys — the lease is lost and `DestroyVm` is never sent.
The slot stays `PAUSED_S` forever.

By the end of the day all 4 worker slots were leaked orphans at identical
icount 641,343,512, and every `RestoreSnapshot` failed with
`ResourceExhausted: NoFreeSlot`.

## How It Was Cleared

`dh-workerd` was restarted on 2026-07-01 (~23:18 UTC) with its exact original
command line; the pid file in the rom-bridge-o73 runtime directory was
updated. Slots are in-memory runtime state, so the restart reclaimed all 4;
nothing durable was touched (snapstore stayed up throughout).

## Division Of Work

- The primary fix is bridge-side and tracked there as
  `rom-operator-bridge-72o`: persist leases (or reconcile/destroy orphaned
  slots on bridge startup).
- Worker-side, consider whether `dh-workerd` wants any of: lease/slot
  expiry, an authenticated admin path to destroy a slot without its lease, or
  at minimum a WARN when `RestoreSnapshot`/`CreateVm` fails with
  `NoFreeSlot` while all slots are paused at the same icount (a strong
  orphan signal). None of this blocks the framebuffer request.
