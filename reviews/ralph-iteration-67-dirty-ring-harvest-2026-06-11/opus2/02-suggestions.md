# Suggestions (non-blocking)

## S1 — `PAGE_SIZE` is now defined twice in dh-vmm; single-source it (item 5)

- **Where:** `crates/dh-vmm/src/dirty.rs:25` (`pub const PAGE_SIZE: u64 = 4096;`) and
  `crates/dh-vmm/src/hash.rs:33` (`pub const PAGE_SIZE: usize = 4096;`).
- **Concern:** Two public `PAGE_SIZE` constants in the same crate, with *different types*
  (`u64` vs `usize`). Today they agree, but a future edit to one won't propagate. The split
  type also means call sites pick whichever import is in scope, which invites confusion.
- **Suggested fix:** Define one canonical `PAGE_SIZE` (e.g. in `kvm.rs` next to
  `DIRTY_RING_ENTRIES`, or a small `consts` module) and re-export / reference it from both
  `dirty.rs` and `hash.rs`. If the two genuinely need different types, derive one from the
  other (`pub const PAGE_SIZE_BYTES: usize = PAGE_SIZE as usize;`) so a single edit is the
  source of truth. Non-blocking — both values are correct now.

## S2 — Document the mid-harvest-error semantics: abort, not recovery (item 8)

- **Where:** `crates/dh-vmm/src/dirty.rs:84-121` (`harvest_into`) and `:184-199`
  (`harvest_at_boundary`).
- **Observation:** `harvest_into` marks each entry `RESET` and advances `next_harvest`
  **incrementally as it drains**. If it errors partway (the `slot != 0` guard at :110, or
  `set.insert` out-of-range at :115), it returns `Err` having already RESET-marked the
  entries *before* the failing one, but `harvest_at_boundary` then short-circuits via `?` and
  **never calls `reset_dirty_rings`**. So those RESET-marked-but-unreaped entries linger in
  the ring.
- **Is that a bug?** No. My scratch experiment confirmed `KVM_RESET_DIRTY_RINGS` is VM-wide
  and stateless w.r.t. our cursor: a later reset reaps any RESET-marked slot regardless of
  which harvest produced it, and DIRTY-but-unharvested entries are never reaped (rc=0 when
  nothing is RESET-marked). The failing entry is a **contract violation** (a memslot id we
  never register, or a GFN past RAM), so on retry `harvest_into` resumes at the same still-
  DIRTY slot and deterministically re-errors. The path is correctly **terminal by design** —
  it is an abort, not a loss, and not a recoverable partial state.
- **Suggested fix:** Add a one-line note on `harvest_into` (or `harvest_at_boundary`) stating
  that an error mid-drain is a hard contract violation: some entries may already be RESET-
  marked-but-unreaped, this is intentionally not cleaned up, and the caller must treat it as
  fatal (no resume). This pre-empts a future snapshot-engine author (bead qmp) wrongly
  building a "retry the boundary" loop around it.

## S3 — Tighten the soft-full wording in the module doc (item 4)

- **Where:** `crates/dh-vmm/src/dirty.rs:11-12` ("...is loss-free by construction — KVM
  cannot overwrite an un-RESET entry; it exits ring-full instead.").
- **Observation:** Modern KVM raises `KVM_EXIT_DIRTY_RING_FULL` at a **soft-full** threshold
  (`kvm_dirty_ring_soft_full`, when used reaches `soft_limit = size - reserved`), i.e. with
  headroom, *before* the ring is literally 100% full. The loss-free reasoning **survives**
  intact — soft-full exits *earlier*, which strengthens the guarantee — but the phrase "it
  exits ring-full instead [of overwriting]" reads as if the ring must be physically full.
- **Suggested fix:** A half-sentence: "…it exits ring-full instead — in practice at a
  soft-full watermark (the kernel reserves headroom), so the exit fires before the ring is
  physically full." Purely a precision/clarity nudge; the guarantee and the code are correct.

## S4 — Confirmed: the forced-tiny-ring determinism test is already filed (item 2)

- **Context:** ARCHITECTURE §8.2 promises the ring-full exit is "verified by a dedicated
  determinism test that forces tiny rings." The ring size is fixed at VM creation
  (`DIRTY_RING_BYTES`), so the existing live tests **cannot** realistically force ring-full
  on a 2 MiB guest with a 65536-entry ring (the committed test acknowledges this in its
  comment). I checked whether a follow-up bead exists — it does, twice:
  - **bead 28i** — "M4 ACCEPT: dirty-ring-full chaos - ring size 512 forced, hashes unchanged
    vs large ring" (P1).
  - **bead v1n** — "CI: nightly chaos jobs - host load, tiny dirty rings, ..." (P2).
- **Suggestion:** No new bead needed. The `DirtyRingFull` classification and the
  `harvest_at_boundary` service-shape are statically correct (the run loop in the live test
  *exercises the service shape* even though it never triggers full), and the forced-tiny-ring
  determinism check is correctly deferred to the M4-accept bead 28i. Optionally cross-
  reference 28i from the §8.2 comment in `dirty.rs` so the "where's the forced-full test?"
  question answers itself.
