# Review Overview — ol1: slot manager + leases

- **Branch:** `ralph/iteration-102-slot-manager-leases-in-dh-worker-slot-ta` vs `main`
- **Date:** 2026-06-13
- **Reviewer:** Claude Opus
- **Bead:** ol1 — the slot manager (ARCH §9 slot table + INTEGRATION §2 leasing)
- **Stats:** 7 files, +1189 / -3, 1 commit

## Summary

This change lands the worker's slot manager: a fixed slot table keyed by
`dh_vmm::SlotState`, the INTEGRATION §2 lease protocol (16-byte tokens gating
every mutating RPC, v1 no-timeout default with a `with_ttl` expiry path behind
it), the R9 fork/CoW-child accounting with auto-thaw, the `reclaim_expired`
reaper, the slot→core pinning map, and the `proto_map` crossings for `Lease` and
`SlotInfo`. It also adds the pinning syscalls (`pin_current_thread` /
`set_current_thread_fifo`) to `dh-vmm::run` (dh-worker forbids unsafe), a
`reset_slot_dirty_tracking` helper for same-slot reuse, a deny-grep test banning
`as i32` outside `proto_map.rs`, and a hardware-gated live test.

The module is **pure bookkeeping** — it owns no KVM resources and is not yet
wired into a daemon (the gRPC service is bead rfv). The whole `pub` surface is
therefore currently uncalled; this is the intended foundation shape, matching the
"INTEGRATION (not yet wired)" note in `dh-vmm/src/lib.rs`.

## State-machine fidelity — the central question

The manager is the prime suspect for state drift, so I traced every write. The
verdict is strong: **every state mutation that lands a new state goes through
`SlotState::transition` / `can_transition`, and every guest-mutating entry point
(`mark_running`, `checkout_write`) composes `ensure_write_path`.** There is no
direct `entry.state = <literal>` write that bypasses the relation except two that
are *provably* legal and one cosmetic synthetic-error case:

- `release()` writes `p.state = Paused` on Frozen→Paused auto-thaw — guarded by
  `p.state == Frozen`, and `Frozen→Paused` is a legal edge. Sound.
- `force_destroy` writes `slots[i].state = Faulted` for children — guarded by an
  explicit `can_transition(Faulted)` check first. Sound.
- `reclaim_expired` writes `slots[idx].state = Faulted` for a Running slot —
  `Running→Faulted` is legal; the guard is the `match` arm itself. Sound.

The lease-precedence ordering (NoSuchSlot → token match → expiry) is correct and
even has a quiet security virtue: a wrong token on an expired lease returns
`StaleLease`, never leaking expiry state to a non-holder. The single-table-mutex
concurrency claim holds — every public method takes the one lock for its whole
critical section; there are no lock-drop-relock windows and no re-entrant locking.

## Findings

No correctness or security defects. The one substantive item is a **fabricated
error value** in `fork`'s CoW-child refusal (an `InvalidTransition{from: Paused,
to: Frozen}` describing a transition that is actually legal) — misleading
diagnostics, not a behavior bug. A handful of suggestions follow. The
`reset_slot_dirty_tracking` doc is honest about its scope and does **not**
overclaim (it resets the KVM ring; it explicitly does not touch the host-side RAM
writes restore_engine performs — see 02). The deny-grep's `Path::ends_with`
semantics are correct (whole-component match, verified empirically).

## Verdict

**APPROVE**

State-machine fidelity is the load-bearing property of this security-adjacent
module, and it holds under scrutiny. Token handling, validation ordering, and the
write-path guard are all correct. The findings are a misleading error value and
quality polish — none block merge.
