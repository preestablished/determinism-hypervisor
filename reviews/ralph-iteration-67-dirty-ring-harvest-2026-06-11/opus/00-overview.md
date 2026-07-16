# Review Overview — Dirty-Ring Harvest (bead ygt)

- **Branch:** `ralph/iteration-67-dirty-ring-harvest` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Files changed:** 3 (`crates/dh-vmm/src/dirty.rs` +413 new, `crates/dh-vmm/src/kvm.rs` +8, `crates/dh-vmm/src/lib.rs` +2); 423 insertions, 0 deletions.

## Summary

This change implements the KVM dirty-ring harvest path of ARCHITECTURE §8.2: a per-vCPU
`KVM_CAP_DIRTY_LOG_RING_ACQ_REL` ring is `mmap`'d off the vCPU fd at
`KVM_DIRTY_LOG_PAGE_OFFSET`, drained with the acquire/release harvest protocol, re-armed
with a raw `KVM_RESET_DIRTY_RINGS` ioctl (kvm-ioctls 0.24 lacks a wrapper), and accumulated
into a dense per-slot `DirtyPageSet` bitmap. `enable_dirty_logging` flips the memslot's
`KVM_MEM_LOG_DIRTY_PAGES` flag, `harvest_at_boundary` is the single drain+reset entry point
(also the `KVM_EXIT_DIRTY_RING_FULL` service path), and `classify_exit` learns
`ExitEvent::DirtyRingFull`. I verified the unsafe ACQ_REL protocol against the kernel ABI and
QEMU's `accel/kvm` reference implementation: the struct field layout (`flags` u32 at offset 0,
`slot` = `as_id|slot_id`, `offset` u64), the acquire-load of DIRTY, the **store-release of
`flags = KVM_DIRTY_GFN_F_RESET`** (replacing, not OR-ing — exactly QEMU's
`dirty_gfn_set_collected`), the free-running cursor, and the loss-free / soft-full claim all
check out. The mmap geometry, off_t, drop munmap, the `0xAEC7` ioctl encoding, and the
single-slot `slot != 0` guard are all correct. The dense-bitmap math is correct and the
divergence from the §8.2 RoaringBitmap sketch is documented. The one substantive issue is a
**documentation contradiction, not a code bug**: ARCHITECTURE §8.2 (and §2.2, line 118) state
`KVM_MEM_LOG_DIRTY_PAGES` is set "only on the bitmap fallback path," but the kernel requires
dirty tracking on the memslot for the *ring* to publish entries too — the implementation is
correct and the doc is stale. This belongs in the divergence-collector bead `veu`.

## Verdict

**APPROVE** — the unsafe systems code is correct against the kernel ABI and the live tests
pass on this box; the only follow-up is a doc-divergence note (non-blocking, tracked in `veu`).

## Stats

- Critical: 0
- Important: 1 (doc divergence — ARCH §8.2/§2.2 wording vs implementation; route to `veu`)
- Suggestions: 5
- Positive notes: 8
- Tests: `cargo test -p dh-vmm --lib` → **84 passed, 0 failed**; the 4 `dirty` tests
  (2 unit + 2 live, incl. the real-mode guest dirtying 0x2/0x5/0x9 + cycle-2) all pass.
  `cargo clippy -p dh-vmm --lib` → clean.
