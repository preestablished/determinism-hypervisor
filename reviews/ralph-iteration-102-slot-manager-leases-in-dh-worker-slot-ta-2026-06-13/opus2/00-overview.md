# Review Overview — Slot Manager + Leases (bead ol1)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-13
- **Branch:** `ralph/iteration-102-slot-manager-leases-in-dh-worker-slot-ta` vs `main`
- **Stats:** 7 files, +1189 / −3, 1 commit (`37b7368`)

## Summary

This change lands the ARCH §9 / INTEGRATION §2 slot manager: a fixed slot table
(`crates/dh-worker/src/slot_manager.rs`) with lease-token-gated mutating
operations, all-or-nothing `fork` with R9 child accounting and last-child
auto-thaw, a caller-supplied-`now_ms` expiry reaper behind a v1 no-timeout
default, the slot→core map, a same-slot dirty-ring reset for restore reuse, new
thread-pinning syscalls in `dh-vmm::run`, proto bridges + a deny-grep test in
`proto_map.rs`, and a hardware-gated live test.

The module is genuinely well-built. The state machine is delegated to a single
source of truth (`SlotState::can_transition` / `ensure_write_path`), every
mutating entry point composes `validate_entry`, time is injected rather than
read, and the 26 unit + 2 live tests cover the important transitions including
all-or-nothing fork and the cross-tenant force-destroy cascade. The lease gate
is real and consistently applied.

My independent scrutiny confirmed the headline correctness claims hold:

- **Lease replay across slot reuse** is defended in practice — `release()` /
  `SlotEntry::empty()` clears the old token, and a re-allocate mints a fresh one
  from `/dev/urandom`. Token *uniqueness* is not enforced (see 02), but a stale
  token only validates if `/dev/urandom` repeats 16 bytes — not a real risk.
- **Wrong-tenant auto-thaw** (parent force_destroyed → slot reused → old child
  destroyed → `release()` decrements the new tenant) is **defended**, but only
  because `force_destroy` clears every child's `parent = None` before reusing the
  slot id. This is a load-bearing, untested-in-isolation invariant (see 01).
- **`fork` all-or-nothing** holds on every early-return `?` path — the parent
  `Frozen` transition is computed but **not committed** until after the free-slot
  count check passes. Verified mutation-by-mutation.
- **`Path::ends_with("proto_map.rs")`**, `mem::zeroed` vs `CPU_ZERO`, and the
  proto field widths are all correct.

I found **one Important** correctness gap (zero-child fork freezes a slot with no
non-destroy exit), and a small set of suggestions around defensive hardening and
documentation of implicit contracts (TOCTOU window, token-in-Debug, the
`parent=None` invariant).

## Verdict

**APPROVE WITH NITS.** No blocking defect. One Important edge case worth fixing
before this module is wired to the daemon; the rest are suggestions. The lease
gate earns its keep.
