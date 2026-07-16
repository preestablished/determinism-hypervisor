# Review — ralph iteration 46: pv-blk base image + CoW overlay fixtures (bead ws4)

- **Branch:** `ralph/iteration-46-cow-fixture`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Spec:** ARCH §6.5 (pv-blk: O_RDONLY base, content hash in MachineConfig, 64 KiB CoW clusters, RMW on first write)

## Summary

This iteration ships the ws4 base-image + overlay fixtures and a host-only
(no-KVM) integration test that drives the real `PvBlk` + `FileBase` through the
MMIO register protocol over a produced 1 MiB base image.

- `tests/nanokernel/src/image.rs` (NEW): deterministic generator (2,048 sectors
  × 512 B), `BASE_IMAGE_BLAKE3` drift-gated constant, `OVERLAY_WRITES`,
  `overlay_sector`, `expected_sector_after_writes`, mirrored geometry constants,
  4 unit tests.
- `crates/dh-vmm/tests/blk_fixture.rs` (NEW, portable): geometry drift gate,
  full-image known-pattern read, dirty-cluster exactness, post-write re-reads,
  base mtime + byte-hash invariance, FileBase pread sanity.
- `crates/dh-vmm/Cargo.toml`: nanokernel dev-dep **un-gated** (was x86_64-only).
- `tests/nanokernel/{Cargo.toml,src/lib.rs}`: + workspace `blake3`, `pub mod image`.

## What I verified by execution

- **blake3 constant independently regenerated** from a hand-retyped copy of the
  generator formula in a *separate* compile unit (not the project's generator):
  `5d22160797753e1ef9844eae15f8d490c702547f34dd6a4fb083598c6c20e85f` — matches the
  `[u8;32]` constant byte-for-byte, length exactly 1,048,576. Byte order vs
  `blake3::hash().as_bytes()` confirmed.
- **aarch64 portability (the highest-risk change): PASSES.** Cross-checked
  `cargo check -p dh-vmm --tests` and `cargo check --workspace --all-targets`
  for `aarch64-unknown-linux-gnu` (clang + llvm-ar-18 + rust-lld). blake3
  (cc/NEON), nanokernel, and the `blk_fixture` test target all compile. No
  `#[cfg]` gate is needed — the test uses no KVM. (CI's arm lane is a *native*
  `ubuntu-24.04-arm` runner building `--workspace --all-targets`, so the fixture
  test really does compile and run there.)
- **CoW contract gap the fixture does NOT cover** — a single chunked request
  mixing a *clean* and a *dirty* cluster — confirmed handled correctly by adding
  a temporary in-tree test (clean → overlay → RMW-fill within one read request),
  running it green, then reverting. Device unit tests cover dirty+dirty crossing;
  this clean+dirty-in-one-request path is not asserted anywhere (see 02).
- **Dirty cluster set {0,1,15}, count 3** re-derived by hand. Phase-3 BATCH=64
  sweep: 2048/64 = 32 exact (no tail skip), full coverage; no batch crosses a
  cluster boundary (64 < 128, aligned); cluster 15's RMW-fill head (1920..2040)
  IS read back and verified against base.
- **Base immutability**: `FileBase` is `File::open` (O_RDONLY) + `read_exact_at`
  (pread, no cursor); struct exposes no write path. mtime check is redundant with
  (and weaker than) the blake3 byte-equality check; both present. Good.
- Ran: `cargo test -p nanokernel` (7 ok), `cargo test -p dh-vmm --test blk_fixture`
  ×5 (stable), `cargo test -p dh-vmm -p dh-devices`, full `cargo test --workspace`
  (KVM present, live tests ran), `cargo clippy --all-targets -D warnings` on the
  three crates. All green. Tree clean.

## Verdict

**APPROVE**

Correct, deterministic, well-documented, and genuinely portable (verified by
cross-check, not asserted). The blake3 fixture hash is independently reproduced.
No Critical or Important issues. Two optional suggestions and several positives.

## Stats

- Files changed: 6 (+345 / −1). 2 new files.
- New tests: 4 (nanokernel) + 3 (blk_fixture) = 7.
- Findings: 0 Critical, 0 Important, 2 Suggestions.
