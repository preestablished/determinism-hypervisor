# Positive Notes

- **Generator math is genuinely asm-cheap and I verified it byte-for-byte.** The
  `& 0xFF` is free (truncate-to-`u8`), and the formula is two `imul`s plus adds. I
  reproduced `base_sector` and `overlay_sector` independently in Python and in
  standalone Rust; both match the in-tree generators exactly
  (`base_sector(5)[8..16] = [176,189,202,215,228,241,254,11]`). The
  `wrapping_mul` → cast-to-`u8` semantics are unambiguous for an asm implementer.

- **Sector-header design is thoughtful.** Base headers = LE sector index (a guest
  verifies a read landed with one qword compare); overlay headers = `!sector` so the
  top bits are set and *cannot* collide with any base header (all base headers are
  `< 2048`). The `sector_headers_are_unique_and_patterns_differ` test proves this
  invariant rather than asserting it in prose — exactly the right discipline.

- **The write set is deliberately adversarial in a small space:** lone sector in
  cluster 0, a write crossing the 0→1 cluster boundary, a *second* write into an
  already-dirtied cluster (RMW-reuse), and the exact image tail ending at capacity.
  Four writes, three dirty clusters {0, 1, 15}, all four CoW edge behaviors. The
  hand-maintained `OVERLAY_EXPECTED_DIRTY_CLUSTERS = 3` is cross-checked by a
  derivation test (`write_set_is_in_capacity_nonoverlapping_and_dirty_count_matches`)
  that recomputes the set, asserts non-overlap and in-capacity, and pins the exact
  `{0,1,15}` — the same drift-gate pattern used elsewhere in the repo. Good.

- **Phase 3 actually checks the RMW-preserves-base property at scale.** It re-reads
  *every* sector (not just written ones) against `expected_sector_after_writes`, so
  the unwritten sectors of dirtied clusters 0, 1, and 15 are verified to still hold
  base bytes — the strongest possible statement of the CoW fill contract.

- **The drift gate on geometry is correct.** `geometry_mirrors_dh_devices` asserts
  `image::IMAGE_SECTOR_SIZE == blk::SECTOR_SIZE` and the per-cluster count, keeping
  nanokernel dependency-light without letting the mirrored constants silently drift.

- **The dev-dep un-gate is well-reasoned and documented in-place.** The comment in
  `crates/dh-vmm/Cargo.toml` explains *why* (the live-KVM ELF tests are already inside
  x86_64-gated modules; the portable CoW fixture must run on arm). I confirmed clippy
  passes on aarch64, so the portable test genuinely compiles on that lane.

- **Hash-as-MachineConfig-fixture is the right call.** `BASE_IMAGE_BLAKE3` is the
  content hash that MachineConfig records, and `generator_is_deterministic_and_hash_gated`
  regenerates and compares so any generator drift fails the test gate. The constant —
  not a file artifact — is what MachineConfig actually consumes, which makes the
  "lib fn vs build.rs artifact" question (see 04) largely moot for correctness.

- **Determinism holds under repetition:** 5 consecutive `blk_fixture` runs produced
  identical results, and the full workspace suite (35 binaries) passed clean.
