# Critical & Important Findings

## Critical

None. The validation is total over hostile input, the slice indexing is inherited from the
already-fuzz-disciplined reader, and no continuity rule has a gap I can exploit to admit a
non-composing lineage. The hostile-lineage shapes I tried (below, in the Important/analysis
section) are all benign because a `Lineage` is a *path*, never a graph.

---

## Important

### I1 — `Lineage` is not `Clone`, but `extend()` consumes `self` — O(children × prefix) blowup on cw2's exact hot path

**Files:** `crates/dh-inputlog/src/splice.rs:58` (`#[derive(Debug)]` — no `Clone`), `:98`
(`pub fn extend(self, ...)`).

`extend()` takes `self` by value and the type is not `Clone`. The named consumer is
`determinism-hypervisor-cw2`: *"root snapshot, 1000 seeded pad-burst children, VerifyReplay
all"* — i.e. ONE parent prefix, 1000 children, each `child = parent_prefix + child_segment`.
That is precisely the shape `extend()` was written for ("the fork-tree composition this
module exists for", doc-comment :95).

With the current API the consumer has exactly two options, both bad:

1. **Rebuild the prefix per child via `new()`** — `Lineage::new(&[parent_seg0, ...,
   child_seg])` for all 1000 children. `new()` calls `LogReader::parse` on *every* segment,
   and `parse` recomputes `blake3::hash(body)` over each segment's full record region
   (`reader.rs:278`). So the parent prefix's body hashes are recomputed 1000 times. Cost:
   **O(children × prefix_segments × prefix_bytes)** of BLAKE3.
2. **Keep one `Lineage` and `extend` it** — impossible past the first child, because
   `extend(self)` *moves* the parent. After `parent.extend(&child_0)` the parent is gone;
   child_1..999 have nothing to extend.

The whole point of `extend()` — "Re-validates only the new edge (the prefix is already
proven)" (:96) — is unreachable for the multi-child case it names, because you can't reuse
the proven prefix without cloning it, and you can't clone it.

**Why this is Important and not a nit:** this is the M7 phase-exit hot path (1000 forks,
P0 bead). The blowup is on the verification critical path the milestone gates on, and the
fix is free.

**Fix (cheapest, recommended):** derive `Clone` on `Lineage`. Every field already is
(`Vec<(&[u8], Header)>`; `Header: Clone`, `&[u8]: Copy`). Then the consumer writes the
prefix once and `parent.clone().extend(&child_i)` per child — O(children) BLAKE3 over the
child segments only, the prefix validated exactly once.

```rust
#[derive(Clone, Debug)]
pub struct Lineage<'a> { /* ... */ }
```

**Alternative (if you want extend to stay consuming for move-chaining):** add an
`extended(&self, child: &'a [u8]) -> Result<Self, SpliceError>` borrowing variant that
clones the prefix `Vec` internally and validates only the new edge. But `derive(Clone)` +
the existing `extend` covers it with less surface.

This is the only finding I would block a clean merge on pending a decision — it is a real,
named-consumer ergonomic regression on a P0 path, even though it is not a correctness bug.

---

## Analysis: hostile-lineage shapes (all benign — recorded so the next reviewer need not re-derive)

These were the adversarial angles I was asked to judge. None is a defect; documenting the
reasoning so the "is this exploitable?" question is answered once.

- **(a) Single segment, self-loop `end == own base`.** Passes today (no inner edges to
  check). Harmless: a one-segment lineage has zero stitch edges, so `end_snapshot_id` is
  never read for stitching; `end_identity()` just reports it. A self-referential end ref on
  a leaf is a content-addressed claim that the end state equals the base state — physically
  it means "replaying this segment returns to the start ref", which `verify_replay` would
  either confirm or flag as `end_state_hash` divergence. Splice-level silence is correct;
  this is a replay-time property, not a continuity-rule violation. **Not a smell.**

- **(b) 2-cycle as a linear list (`seg0.end==seg1.base` AND `seg1.end==seg0.base`).**
  Passes — and correctly so. A `Lineage` is an *ordered path*, not a graph; cycles only
  matter to a builder that treats refs as graph nodes. As a path `[seg0, seg1]` this is a
  valid two-edge induction: replay seg0 from base0 → end0; replay seg1 from end0(=base1) →
  end1(=base0). Whether end1 actually equals base0 is a replay-time fact `verify_replay`
  checks. The splice layer correctly does not reason about graph cycles. **Fine.**

- **(c) Duplicate segments (same bytes twice).** Only validates if `seg.end == seg.base`
  (self-anchored), which reduces to case (a) repeated. Each duplicate is independently
  re-parsed and re-stitched; no aliasing issue (segments are `&[u8]`, read-only). **Fine.**

- **(d) Zero base mid-lineage.** Impossible via the stitch rule: an inner segment's
  predecessor must have a *non-zero* end (`MissingEndSnapshot` rejects zero inner end,
  :85/:104), and the stitch requires `next.base == prev.end != 0`. So no mid-lineage
  segment can have a zero base. **Structurally precluded — good.** The *root* base CAN be
  zeros (boot segment); see S1 in 02-suggestions — that is legal-by-design but undocumented.
