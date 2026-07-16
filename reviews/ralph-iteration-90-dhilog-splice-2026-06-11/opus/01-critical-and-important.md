# Critical and Important Findings

## Critical

None. The continuity model is sound and the case analysis is complete.

---

## Important

### I1. `extend(self)` consumes the prefix — wrong signature for the named consumer (cw2's 1000-children hot path)

**File:** `crates/dh-inputlog/src/splice.rs:98`

```rust
pub fn extend(self, child_segment: &'a [u8]) -> Result<Self, SpliceError> {
```

The module's own doc-comment names `extend` "the fork-tree composition this module exists
for" and frames per-child lineage as `parent prefix + child segment` — i.e. **one shared
root/parent prefix plus one fresh child segment per fork**. The named consumer is
`determinism-hypervisor-cw2`: "boot → root snapshot → **1000** tier-A forks ... VerifyReplay
every (snapshot, spliced log)". So the realistic call shape is one root prefix reused across
1000 children.

`extend(self)` takes the receiver **by value**, so each call destroys the prefix it builds
from. To produce 1000 child lineages from one root, the caller is forced into one of:

1. **Re-validate the root 1000 times:** `Lineage::new(&[&root])?.extend(&child_i)?` in a
   loop — re-parses and re-validates the root segment (full `LogReader::parse`: header +
   body_hash BLAKE3 over the whole root body + record walk) on every iteration. For a
   1-guest-second root that is real, repeated, avoidable work in the M7 hot path.
2. **Clone before extend:** there is no `Clone` on `Lineage` and no `&self` variant, so this
   isn't even available without an API change.

The prefix that would need cloning is `Vec<(&'a [u8], Header)>` — borrowed byte slices plus
already-parsed `Header`s. Cloning it is **cheap** (no re-parse, no re-hash; just a Vec of
fat pointers + fixed-size headers). So the fix is low-cost and high-value:

**Recommended fix** — offer a non-consuming fork that re-validates only the new edge:

```rust
pub fn extend(&self, child_segment: &'a [u8]) -> Result<Lineage<'a>, SpliceError> {
    // ... same edge validation against self.segments[last] and self.segments[0] ...
    let mut segments = self.segments.clone(); // cheap: &[u8] + Header
    segments.push((child_segment, h));
    Ok(Lineage { segments })
}
```

(Requires `#[derive(Clone)]` on `Lineage`, which is free — `Header` already derives `Clone`,
confirmed in `reader.rs:84`.) If a consuming variant is still wanted for the linear-append
case, keep it as `into_extended(self, ...)`; but the `&self` form should be the default,
because the stated consumer is a 1-to-many fan-out, not a linear chain.

**Why Important, not Critical:** this is correctness-neutral — the per-segment verification
still holds whichever way the caller builds the lineage. It is ranked Important only because
the consumer is *named in the bead* and the current signature pushes that consumer onto the
re-validate-1000×-root path with no cheap alternative in the public API.
