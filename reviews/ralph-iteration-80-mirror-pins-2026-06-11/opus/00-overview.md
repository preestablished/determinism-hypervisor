# Review: ralph/iteration-80-mirror-pins

- **Branch:** `ralph/iteration-80-mirror-pins`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** determinism-hypervisor-sr5 — proto↔domain enum mirror pins
- **Commit under review:** `559062e` (iteration 80 checkpoint - proto/domain mirror pins)

## Summary

This change closes the gap between two enum families that were each individually
frozen but never asserted to *agree*: the domain enums (`dh_vmm::SlotState`,
`dh_vmm::runctl::StopReason`) and the proto wire enums (`dh_proto::v1::SlotState`,
`dh_proto::v1::StopReason`). The families intentionally disagree in both offset
and order — domain `SlotState::Running` is discriminant 1 while proto `RUNNING = 3`,
proto reserves 0 for `*_UNSPECIFIED` that no domain enum carries, and `Paused`/`PAUSED_S`
agree at 2 only by coincidence — so a naive `as i32` cast across the seam would
silently mislabel slot state on the API.

Three things land:

1. **`crates/dh-worker/src/proto_map.rs`** (new) — hand-written `slot_state_to_proto`
   (ungated, all-arch) and x86-gated `stop_reason_to_proto`, both exhaustive matches.
   Unit tests pin every arm to its exact proto wire number and assert the
   "4-of-5 casts lie" trap, demonstrating *why* the module exists rather than just
   that it works.
2. **`crates/dh-inputlog/tests/stop_reason_mirror.rs`** (new, with a `dh-proto`
   dev-dep) — proves every proto `StopReason` fits the END record's `u8` slot and
   that the golden fixtures' frozen `stop_reason` bytes decode to the *intended*
   proto variants (kitchen-sink → `GOAL_SATISFIED=2`, minimal → `STOP_UNSPECIFIED=0`),
   turning the API.md §3.3 "mirrors proto StopReason" prose into a checked fact.
3. **`crates/dh-worker/src/lib.rs`** — registers `pub mod proto_map` (ungated
   module, x86-gated fn inside).

The work is well-targeted, the tests are genuinely load-bearing (not tautological),
the architecture-gating is correct, and I verified all four new tests pass on
x86_64 and that `dh-worker` clippy is clean on `aarch64-unknown-linux-gnu`. The
one open judgment call is whether the iteration-79 extension's *explicit* ask to
"grep-forbid `as i32` casts on SlotState" is adequately discharged by
convention-by-doc + the lying-casts pin, or whether a deny-grep gate (the
`no_host_ambient_authority` pattern already in `dh-devices`) should be added. I
treat that as an Important-but-non-blocking follow-up, not a defect in what landed.

## Verdict

**APPROVE**

The change fully discharges the original sr5 scope (END `u8` ↔ proto mirror via
golden fixtures + u8-fit) and the substantive half of the iteration-79 extension
(hand-written `SlotState` conversion + wire pin). The "cast ban" half is satisfied
in spirit by routing every crossing through the function and pinning the lie, but
the literal grep-forbid was not added. Because there are currently **zero** `as i32`
casts on `SlotState` anywhere in the tree and the first consumer (ol1) does not yet
exist, the missing grep gate is a hardening follow-up rather than a correctness gap.
I recommend filing it (see action items) but do not consider it a merge blocker.

## Stats

- Files changed: 4 (+ `Cargo.lock`)
- New files: 2 (`proto_map.rs`, `stop_reason_mirror.rs`)
- Diff: ~198 lines, 1 commit
- New tests: 4 (2 in `proto_map`, 2 in `stop_reason_mirror`) — all verified passing
- New dependency edges: `dh-inputlog` dev-dep → `dh-proto`
- Findings: 0 Critical, 1 Important (follow-up), 4 Suggestions, 6 Positive notes
