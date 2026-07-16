# Iteration 84 — dirty-ring chaos (bead 28i, M4 ring-chaos ACCEPT) — 2nd review

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-84-dirty-ring-chaos`
- **Scope:** ~388 diff lines — `create_slot_vm_with_ring` + `SlotVm::dirty_ring_entries` + `DirtyRing::map_sized` plumbing; `page_dirtier` guest (3072 pages); `ring_chaos.rs` two-leg acceptance test; `elf_shape` + nanokernel lib additions.

## Summary

This is a clean, well-reasoned acceptance test for risk R8 (dirty-ring-full loss-free servicing). The two-leg design — identical guest on a 65536-entry ring (never overflows) vs a 1024-entry ring (overflows ≥2×), asserting identical incremental snapshot refs + `pages_shipped` + bit-equal vCPU — is the right shape: a single lost dirty page would perturb the delta manifest and break ref equality. The non-vacuity pins (`large.ring_full_exits == 0`, `small.ring_full_exits >= 2`) close the "did the stressor actually fire" gap that catches most chaos tests. The plumbing (`map_sized`, `create_slot_vm_with_ring`, the `% self.entries` cursor mask) is minimal and correct, and the inline doc comments are unusually honest about the two empirical constraints (the 64+512 PML floor forcing 1024, and the 0vl store hang capping at 16MiB).

I verified every adversarial concern raised in the brief and found **no correctness bug**. The cursor mask cannot overflow (`u64` free-running, ~3500 of 2^64 harvests), termination reads per-entry `flags & DIRTY` correctly for a partially-filled ring at any wrap position, the post-Hlt tail is drained by `take_snapshot`'s own `harvest_at_boundary` (snapshot_engine.rs:133), and `0u64.is_power_of_two()` is `false` so a zero ring is correctly rejected. The findings below are all maintainability / drift-protection, not behavior.

## Verdict

**ACCEPT.** No Critical or Important blocking issues. One Important-class maintainability hardening (the `map_sized`↔`SlotVm` decoupling footgun) and two Suggestions (dead `DIRTY_RING_BYTES` const + duplicated `16` literal; missing asm-drift pin for `page_dirtier`, which leaves `PAGE_DIRTIER_START_GPA` dead). None of these gate the merge.

## Stats

| Severity    | Count |
|-------------|-------|
| Critical    | 0     |
| Important   | 1     |
| Suggestions | 2     |
| Positive    | 4     |

## 0vl filing judgment

`determinism-hypervisor-0vl` (P1 BUG, 32MiB FULL snapshot hangs the blocking client in ep_poll) is **well-filed**: clear repro, scoped suspicion (tonic max-message / h2 flow-control in the sibling put path), and it correctly calls out the 9sb impact in prose ("MATTERS FOR 9sb: the perf acceptance plans a 128MiB guest — it will hit this"). However it does **not** carry a formal `bd dep` edge blocking 9sb, and it should — see 01-critical-and-important.md.
