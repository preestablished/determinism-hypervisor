# Positive notes

- **The blake3 fixture hash is genuinely reproducible.** Re-deriving it from a
  re-typed generator in a separate compile unit produced the exact constant.
  The drift-gate unit test (`generator_is_deterministic_and_hash_gated`) makes
  any future generator change fail the build, which is exactly what you want for
  a value destined for MachineConfig.

- **Portability claim is backed by a real cross-build, and it is correct.**
  Un-gating the nanokernel dev-dep (the iteration's riskiest change) survives
  `cargo check --workspace --all-targets` for aarch64. The accompanying comment
  in `crates/dh-vmm/Cargo.toml` correctly explains *why* it is safe (KVM tests
  are in cfg-gated modules; the fixture test uses none). The reasoning matches
  reality.

- **The overlay fixture is deliberately adversarial in the right places.**
  `OVERLAY_WRITES` packs a lone sector, a cluster-boundary-crossing write, a
  second write into an already-dirtied cluster (RMW-reuse), and the exact image
  tail ending at capacity — covering the four distinct CoW code paths in one
  small set. The "exact image tail" entry (2040..2048) also exercises the
  capacity-edge of `request_range`.

- **`expected_sector_after_writes` encodes the CoW invariant precisely:**
  overlay where written, base everywhere else *including the unwritten
  remainder of dirtied clusters* — so the test's phase 3 actually proves RMW
  preserved base bytes, not merely that writes landed.

- **Overlay header (`!sector`) cannot collide with any base header** by
  construction, and the unit test asserts it. This makes "did the read land in
  overlay vs base?" decidable from the first 8 bytes alone — a clean
  asm-verifiable design for the future guest-side consumer.

- **Documentation quality is high.** Both new files carry accurate module docs
  tying each fixture/assertion back to ARCH §6.5 and explaining the drift-gate
  discipline; the dev-dep comment is precise. No overclaiming spotted.

- **No determinism leak introduced.** `temp_path` avoids host randomness; the
  generator is pure; the test asserts `dirty_clusters()` exactness rather than
  ">= something". Re-ran the fixture test 5× with bit-stable results.

- **Reuses the established register-protocol test harness** (matching the
  `request()` helper already in `blk.rs` and `blkfile.rs`), keeping the
  device-driving boilerplate consistent across the codebase.
