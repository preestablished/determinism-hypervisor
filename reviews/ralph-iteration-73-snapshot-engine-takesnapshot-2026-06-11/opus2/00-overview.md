# Snapshot Engine `take_snapshot()` — Review (2nd reviewer)

- **Branch:** `ralph/iteration-73-snapshot-engine-takesnapshot` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** qmp — TakeSnapshot orchestration (ARCH §8.2)
- **Scope:** `crates/dh-worker/src/snapshot_engine.rs` (new, 288 lines),
  `crates/dh-worker/tests/snapshot_engine.rs` (new, 361 lines), Cargo wiring.

## Summary

The engine orchestrates one TakeSnapshot end-to-end: gate the §8.1
preconditions (agenda empty + slot Paused), build the page set (full walk
or dirty-ring drain), ship bare pages to the REAL snapshot-store through
the blocking facade, assemble the DHSNAP device blob in the canonical §4
section order, `PutSnapshot`, and return the ref **only** after durable ack
— clearing the dirty set as the last step. It reaches the store via
`snapstore_client::blocking::SnapstoreClient`, the sibling's sync/async
bridge; the joint tests spawn the real server in-process (R12).

I ran independent experiments rather than re-checking the author's
assertions. The headline property — **byte determinism of the ref** — was
verified empirically, not just by code reading:

1. **Same quiescent slot, two `Full` snapshots → IDENTICAL ref**
   (`f2fddfb4…`). No nondeterministic section. ✓
2. **Cross-VM identity**: two *independently constructed* slots+buses with
   the same seed/config → **the same ref** (`f2fddfb4…`). The fork/dedup
   foundation holds through the entire engine, not just the VCPU capture —
   KVM reserved-byte zeroing (iteration-70 audit) survives the full path. ✓
3. **Empty delta** (incremental retry with a cleared/never-dirtied set):
   ships 0 pages and produces a **valid** DELTA manifest (`parent=Some`,
   `entries=0`) — no error, no guard needed. The contract is just
   undocumented. ✓ (see 02)
4. **BLKO**: a `PvBlk` registered out of base order still lands in the
   canonical slot (`…PADD, BLKO, SERL`) and round-trips through the
   container. The §4-order claim holds for the device with a real
   non-empty section. ✓
5. **`agenda_empty` attestation**: there is no live, queryable agenda
   object after `run_segment` returns; the bool is the honest seam today,
   though `icount/vns/hash_chain` could be sourced more rigorously from
   `SegmentOutcome` once ol1 owns the run handle. (see 02)

`cargo test -p dh-worker --test snapshot_engine` passes twice
(deterministic), `cargo clippy -p dh-worker --tests` is clean, working tree
clean after experiments (all scratch reverted).

## Verdict

**APPROVE.** No Critical or Important findings. The core determinism
property is empirically solid (including the harder cross-VM case the
fork/dedup roadmap depends on). The one finding worth acting on is a
non-blocking efficiency issue: pages are uploaded twice (step 3 +
`put_snapshot_from_parts` internally re-uploads). The rest are
documentation/robustness suggestions.

## Stats

| Item | Count |
|---|---|
| Critical | 0 |
| Important | 0 |
| Suggestions | 4 |
| Positive notes | 6 |
| Experiments run | 5 (all confirming) |
| Tests (official) | 4 pass × 2 runs, deterministic |
| Clippy | clean |
| Tree after review | clean |
