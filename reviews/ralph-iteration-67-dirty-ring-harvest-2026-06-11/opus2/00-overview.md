# Review Overview — Dirty-Ring Harvest (bead ygt) — Second Reviewer

- **Branch:** `ralph/iteration-67-dirty-ring-harvest` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Files changed:** 3 — `crates/dh-vmm/src/dirty.rs` (+413, new),
  `crates/dh-vmm/src/kvm.rs` (+8), `crates/dh-vmm/src/lib.rs` (+2). 423 insertions, 0 deletions.

## Summary

Independent second review. Rather than re-deriving correctness from the kernel ABI (the
first reviewer did that against the QEMU `accel/kvm` reference), I ran **experiments against
`/dev/kvm` on this 6.8.0-124 box** to settle the open questions empirically, then assessed
the integration/error surface statically.

What I ran (scratch tests appended to `dirty.rs`, run, then reverted — nothing committed):

1. **enable_dirty_logging necessity (settles the veu divergence empirically).** A copy of the
   live guest-write test with `enable_dirty_logging` **omitted** harvested **0** entries
   (`set.contains(0x2) == false`). The unmodified live test, which sets the flag, harvests
   the written pages. Clean A/B: `KVM_MEM_LOG_DIRTY_PAGES` **is required for ring publication**
   on 6.8 — the code is correct, ARCHITECTURE §8.2's "flag only on the bitmap path" wording is
   wrong. (Same conclusion as reviewer 1's ABI argument; I add the direct experimental proof.)

2. **Harvest idempotence / double-reset (item 3).** Two back-to-back empty
   `harvest_at_boundary` calls returned `HarvestStats::default()` both times (cursor stable);
   two back-to-back raw `KVM_RESET_DIRTY_RINGS` with nothing collected both returned `rc=0`.
   Safe and idempotent.

3. **reset-without-harvest (item 8 probe).** Drove a guest write so the kernel marks the ring
   entry DIRTY, then called `reset_dirty_rings` **without harvesting** (no entry marked RESET):
   `rc=0`. Confirms the kernel reaps only RESET-marked slots; DIRTY-but-un-harvested entries
   are never lost by a reset.

Static assessment covered: the soft-full loss-free reasoning (item 4), `PAGE_SIZE`
duplication (item 5), the `AtomicU32` cast alignment (item 6), the cursor wrap (item 7), and
the partial-harvest-failure recovery story (item 8). All builds/tests/clippy green.

I did not mirror reviewer 1. The shared finding (veu doc divergence) I confirmed empirically
and defer to their write-up; my own findings target the **mid-harvest error recovery path**,
the **PAGE_SIZE single-source concern**, **soft-full wording precision**, and confirming the
**forced-ring-full determinism test is already filed** (beads 28i, v1n).

## Verdict

**APPROVE.** The unsafe systems code is correct and live-verified empirically on this kernel.
No Critical or Important code findings. Two non-blocking suggestions (PAGE_SIZE single-source;
document the mid-harvest-error abort semantics) plus doc/test follow-ups already tracked in
existing beads (veu, 28i, v1n).

## Stats

- Critical: 0
- Important: 0 (the veu doc divergence is real but already raised by reviewer 1 and tracked)
- Suggestions: 4
- Positive notes: 7
- Experiments run: 3 live KVM scratch tests (all confirmed expected behavior)
- Tests: `cargo test -p dh-vmm --lib` → **84 passed, 0 failed**.
  `cargo clippy -p dh-vmm --lib` → **clean (exit 0)**.
