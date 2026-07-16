# Critical & Important Findings

## Critical

**None.** The unsafe KVM code is correct and was live-verified on this 6.8 box:

- `mmap` geometry (`KVM_DIRTY_LOG_PAGE_OFFSET=64` × `PAGE_SIZE` off_t, `DIRTY_RING_ENTRIES ×
  size_of::<kvm_dirty_gfn>()` length), the ACQ_REL harvest (acquire-load of `DIRTY`,
  store-**release** of `RESET` — replacing not OR-ing), the free-running cursor, the
  `0xAEC7` `KVM_RESET_DIRTY_RINGS` encoding, the `Drop` munmap, and the `slot != 0` guard
  all behave correctly in the passing live tests and my scratch experiments.
- The `AtomicU32` cast (item 6) is sound: `kvm-bindings 0.14.0` asserts `kvm_dirty_gfn` is
  `align 8`, `flags` at offset 0 — so `flags` is 4-byte aligned (AtomicU32's requirement is
  satisfied a fortiori), and the cast is `x86_64`-gated (`lib.rs` `#[cfg(target_arch =
  "x86_64")] pub mod dirty;`).
- The cursor wrap (item 7) is correct: `next_harvest: u64` free-running with
  `% DIRTY_RING_ENTRIES`; at the publication rate of a single vCPU, u64 cannot wrap in any
  realistic runtime, and `idx < DIRTY_RING_ENTRIES (65536)` so `as usize` is lossless and
  in-bounds on every target.

## Important

**None (no code-level Important findings).**

For completeness, the one Important-tier issue in this change — the ARCHITECTURE §8.2 / §2.2
wording that `KVM_MEM_LOG_DIRTY_PAGES` is set "only on the bitmap fallback path" — is a
**documentation divergence, not a code defect**, and was already raised in full by the first
reviewer (their I1, routed to bead `veu`). I independently **confirmed it by experiment** on
this kernel rather than by ABI inference:

- **Experiment (scratch, reverted):** a copy of `guest_writes_are_harvested_and_ring_resets`
  with the `enable_dirty_logging(&slot)` call removed harvested **0** entries
  (`harvested=0`, `set.contains(0x2)=false`). The committed live test, which keeps the flag,
  harvests the written pages. This is a direct A/B proof that the flag is required for ring
  publication on 6.8 — the code's `enable_dirty_logging` is correct; the doc text is stale.

No code change is warranted; this is tracked in `veu`. I do not duplicate reviewer 1's
suggested doc rewording here.
