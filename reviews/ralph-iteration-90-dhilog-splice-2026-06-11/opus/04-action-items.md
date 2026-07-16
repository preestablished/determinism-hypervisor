# Action Items

### Critical

None.

### Important

- [ ] **Add a non-consuming `extend(&self) -> Lineage<'a>` (and `#[derive(Clone)]` on
  `Lineage`).** The named consumer cw2 builds 1000 child lineages from one shared root
  prefix; the current `extend(self)` (`splice.rs:98`) consumes the prefix, forcing the caller
  to re-run full `LogReader::parse` on the root 1000× (`Lineage::new(&[&root])` per child).
  Cloning the prefix `Vec<(&[u8], Header)>` is cheap (borrowed slices + already-parsed
  headers, no re-parse/re-hash). `Header` already derives `Clone` (`reader.rs:84`), so this is
  a low-cost change. Keep a consuming `into_extended(self, ...)` too if the linear-append case
  is wanted, but make `&self` the default. Correctness-neutral; ergonomics/efficiency for the
  stated hot path.

### Suggestions

- [ ] **Replace the hardcoded `is_empty() { false }` with `self.segments.is_empty()`**
  (`splice.rs:126-128`) so the result derives from state and can't drift if a future
  constructor is added. (S1)

- [ ] **Optionally cache the `LogReader` instead of re-parsing in `edges()`** — store
  `LogReader<'a>` (it derives `Clone`) in `segments` rather than `Header`, making `edges()` a
  pure clone and removing the only `expect` in the public path. Negligible at cw2 scale; do
  this only if a future caller iterates `edges()` repeatedly on long lineages. (S2)

- [ ] **Add one sentence to the module doc-comment stating `entropy_seed` is a per-segment
  replay input, not a lineage invariant** (a nonzero mid-lineage re-seed is a legal fork
  choice — cw2 runs "DISTINCT seeded" children). Confirmed correct against API.md §3.1/§3.4;
  the note just pre-empts the "should this be checked?" question. (S3)

- [ ] **Add a half-sentence that `encoder_fingerprint` is likewise per-segment** (verifiers
  compare it per segment per API.md §3.1 / `reader.rs:100-103`), so cross-version segments are
  legal and intentionally not gated at the lineage level. (S4)
