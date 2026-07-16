# Critical and Important findings

## Critical

**None.**

## Important

**None.**

---

## Notes on things specifically interrogated and found sound

These are not findings — they are the adversarial probes that came back clean,
recorded so the next reviewer need not re-run them.

1. **blake3 content-hash constant is correct and byte-order-correct.**
   Re-derived in an independent compile unit (re-typed formula, not the project
   generator) against the workspace blake3 rlib:
   `5d22160797753e1ef9844eae15f8d490c702547f34dd6a4fb083598c6c20e85f`. Matches
   `BASE_IMAGE_BLAKE3` and `blake3::hash().as_bytes()` ordering. Image length is
   exactly 1 MiB. The unit test's drift gate is real (it would catch any
   generator change), and the constant is right.

2. **aarch64 build is not broken by un-gating the nanokernel dev-dep.**
   `cargo check --workspace --all-targets --target aarch64-unknown-linux-gnu`
   passes (clang/llvm-ar-18/rust-lld). The KVM-touching dh-vmm/dh-cli modules
   remain `cfg(target_arch="x86_64")`-gated; `blk_fixture.rs` uses no KVM, so no
   gate is required. blake3 compiles for arm (cc/NEON). Confirmed by execution,
   not by reading.

3. **Base file immutability holds by construction.** `FileBase::open` →
   `File::open` (O_RDONLY); reads via `read_exact_at` (pread). No `O_APPEND`,
   no truncate, no `write_at`, no exposed mutator. mtime-equality assertion is
   redundant with the blake3 byte-identity assertion (which is the strong one);
   both are present (`crates/dh-vmm/tests/blk_fixture.rs:108-121`).

4. **Dirty-cluster derivation {0,1,15}, count 3** matches `OVERLAY_WRITES` by
   hand and is independently asserted in
   `tests/nanokernel/src/image.rs:160` (`write_set_is_in_capacity_...`).

5. **Phase-3 coverage is complete.** BATCH=64 over 2048 sectors = 32 exact
   reads, no tail skip; cluster 15's RMW-fill head (sectors 1920..2040) is
   re-read and checked against base; every sector verified against
   `expected_sector_after_writes`.

6. **Overlay-header non-alias claim is precise.** Overlay header = `!sector`
   (≥ 2^64 − 2048); base headers are < 2048. The doc/comment claim is about the
   8-byte header at offset 0 only, and the consumption test compares whole
   sectors, so the (irrelevant) possibility of a base *body* containing a
   matching 8-byte run does not weaken anything.
