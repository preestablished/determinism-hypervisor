# Review: snapshot-engine TakeSnapshot (bead qmp)

- **Branch:** `ralph/iteration-73-snapshot-engine-takesnapshot` vs `main`
- **Head commit:** `43f6abc` (ralph: iteration 73 checkpoint — snapshot engine: TakeSnapshot orchestration)
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Diff:** `/tmp/ralph73-diff.txt` (725 lines)

## Summary

This iteration lands `crates/dh-worker/src/snapshot_engine.rs::take_snapshot()` — the
M4 centerpiece and the first consumer of the d2p `SlotState::Paused` gate. The engine
implements the ARCHITECTURE §8.2 path faithfully: it gates on the two §8.1 preconditions
(agenda-empty attestation + Paused), harvests the dirty ring at the boundary for the
incremental path, reads pages bare from the live mapping, ships them to the **real**
snapshot-store, assembles the DHSNAP container in a *fixed* canonical §4 order (decoupled
from bus iteration order via a `KNOWN_TAGS`-position sort), drives `PutSnapshot` to get the
durability-receipt ref (R12), and — critically — clears the dirty set **only** after the
store ack. The error ordering is correct for retry-safety: a failed `put_pages` or
`put_snapshot` leaves the dirty set intact and the pages idempotently re-uploadable. The
hash-vs-section reconciliation (iteration-70 option (b)) is documented in the module header
and is the right call. Four joint tests (live KVM slot + real in-process store) pass on this
box. The implementation is clean (no clippy warnings), well-documented, and architecturally
sound. The findings are about **a redundant double page-upload** (efficiency, Important) and
**test-coverage gaps** for the byte-determinism and multi-device-ordering claims the code
explicitly makes — no correctness defects.

## Verdict

**APPROVE** (with one Important efficiency fix recommended before the engine is wired into
the hot path, and three follow-up test/doc beads filed).

## Stats

| Metric | Value |
|---|---|
| Files changed | 6 (4 build/manifest, 2 new source) |
| New production code | `snapshot_engine.rs` (288 lines) |
| New test code | `tests/snapshot_engine.rs` (361 lines, 4 tests) |
| Tests run | `cargo test -p dh-worker --test snapshot_engine` → 4 passed, 0 failed |
| Clippy | `cargo clippy -p dh-worker --tests` → clean |
| Critical findings | 0 |
| Important findings | 1 (redundant double page upload) |
| Suggestions | 5 |
| Action items | 4 |
