# Review — iteration 84: dirty-ring-full chaos (bead 28i, R8)

- **Branch:** `ralph/iteration-84-dirty-ring-chaos`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Commit under review:** `86fa6ad` ("ralph: iteration 84 checkpoint — M4 ACCEPT dirty-ring-full chaos (R8)")
- **Base:** `main`

## Scope

One commit, ~249 added / 5 removed lines across 7 files. The change parameterizes the
per-vCPU dirty ring size so a chaos test can force a *tiny* ring and prove that
servicing `KVM_EXIT_DIRTY_RING_FULL` mid-run loses no dirty page (ARCH §8.2 / risk R8):

1. **`crates/dh-vmm/src/kvm.rs`** — `create_slot_vm_with_ring(mem_bytes, ring_entries)`
   (power-of-two validated); `create_slot_vm` delegates with `DIRTY_RING_ENTRIES=65536`;
   the fork path passes the default; `assemble_slot_vm` gains a `ring_entries` arg and
   sizes the cap as `ring_entries * 16`; new `SlotVm::dirty_ring_entries` field.
2. **`crates/dh-vmm/src/dirty.rs`** — `DirtyRing::map_sized(vcpu, entries)`; `map`
   delegates with the default; new `entries` field drives both the mmap length and the
   harvest cursor mask (`next_harvest % self.entries`).
3. **`tests/nanokernel/asm/page_dirtier.asm`** (+ build/lib/shape wiring) — a guest that
   dirties 3072 consecutive pages then parks in HLT.
4. **`crates/dh-worker/tests/ring_chaos.rs`** — two legs differing only in ring size
   (65536 vs 1024); asserts identical root refs, identical delta refs, identical
   `pages_shipped` (≥3072), bit-equal vCPU capture, and non-vacuity (large ring 0
   overflows, small ring ≥2).

## Summary judgement

The plumbing is clean, minimal, and correct. The free-running cursor with the new
`% self.entries` mask is sound across arbitrary wraps (the cursor never rewinds, and
`entries` always matches the mmap length and the kernel-enabled ring size on every
constructed `DirtyRing`). `harvest_at_boundary` is the documented loss-free §8.2
service path and is correctly used on `DirtyRingFull`.

On acceptance honesty: the landed delta-ref-equality assertion is **equal-or-stronger**
than the bead's "hashes must equal" intent. `SnapshotRef` is a BLAKE3 content digest
over the whole manifest body, and the incremental path folds the DHSNAP device blob
(which contains the vCPU/device state) into that body — so ref equality already
discharges page-content, page-index, *and* vCPU/device byte-determinism in one shot. The
extra `assert_eq!(vcpu)` is redundant-but-harmless localization. This is a content-equality
proof, **not** a restore-and-replay roundtrip; that is an acceptable (arguably superior for
R8's "no page lost" claim) discharge, but the deviation from the bead/plan's "roundtrip /
H1==H2" wording should be made explicit (see 01).

Two findings hold it back from a clean APPROVE: (a) the doc trail is internally
inconsistent — the IMPLEMENTATION-PLAN, the bead title/description, and ARCH §8.2 still
say "ring size 512" / "65536" while the test legitimately uses 1024 (kernel floor) — the
512→1024 empirical is recorded only in the test's own doc comment, not reconciled with the
canonical docs; and (b) `DirtyRing::map`/`map_sized` takes no `&SlotVm`, so nothing
*structurally* prevents a future caller from mapping a default-sized ring over a
custom-ring slot and silently mis-masking the cursor. Both are addressable without
reworking the design.

## Verdict

**NEEDS_DISCUSSION** — the code is correct and the acceptance is honest *in substance*,
but the doc/bead reconciliation (Important) and the map/slot-size consistency footgun
(Important) should be resolved or consciously waived before this is treated as the R8
discharge of record. No Critical defects.

## Stats

| Metric | Value |
|---|---|
| Files changed | 7 |
| Lines +/- | +249 / −5 |
| Critical findings | 0 |
| Important findings | 3 |
| Suggestions | 5 |
| Positive notes | 6 |
