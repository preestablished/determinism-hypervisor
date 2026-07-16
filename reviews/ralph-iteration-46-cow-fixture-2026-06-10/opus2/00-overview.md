# Review: pv-blk base image + CoW overlay fixtures (ws4)

- **Branch:** `ralph/iteration-46-cow-fixture`
- **Base:** `main` (`git diff main...HEAD`, single commit `53932bd`)
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** determinism-hypervisor-ws4 (consumer: determinism-hypervisor-40q, M1 acceptance)
- **Spec:** ARCH §6.5

## Summary

This iteration adds the ws4 fixture layer: a deterministic 1 MiB / 2048-sector base
image generated in `tests/nanokernel/src/image.rs` with a drift-gated blake3 content
hash (the MachineConfig fixture), an M1 overlay write set, and a host-only no-KVM
consumption test (`crates/dh-vmm/tests/blk_fixture.rs`) that drives the real
`PvBlk` + `FileBase` end to end over the produced file. The dh-vmm `nanokernel`
dev-dep was un-gated from x86_64 so the portable test runs on the arm lane too.

The work is correct, well-documented, and the test discipline is excellent. I
independently re-derived the byte generators in both Python and standalone Rust and
they match the in-tree generators byte-for-byte (`base_sector(5)[8..16]` =
`[176,189,202,215,228,241,254,11]`; `overlay_sector(3)[0..8]` header = `0xFF...FC`).
I verified the boundary math: the `(2040,8)` tail write ends at `end_sector == 2048
== capacity`, which is STATUS_OK by the intentional `>` check at `blk.rs:145`, and
cluster 15 is fully in-bounds (no zero-fill-past-EOF). The 64-sector batch (32 KiB)
fits the 64 KiB guest buffer with margin and `2048 % 64 == 0` so there is no partial
final batch.

No correctness defects found. My findings are all SUGGESTION / NEEDS_DISCUSSION
level: (1) the formula constants and a `2048 % BATCH == 0` invariant are implicit and
would help the eventual asm guest implementer and future maintainers if made
explicit; (2) the bead text says "script/build step" but production is a lib fn with
no CLI/script entry point — acceptable given the hash constant is the real
MachineConfig input, but worth a follow-up bead for a `dh-cli image` subcommand; (3)
bead 40q (the M1 consumer) does not yet reference these fixtures.

## Verification (all run on the lab box, not eyeballed)

- `cargo test -p nanokernel` — 7+ tests pass (incl. all `image::tests`)
- `cargo test -p dh-vmm --test blk_fixture` x5 — 3/3 pass, identical every run
- `cargo test --workspace` — exit 0, 35 test binaries, zero failures
- `cargo clippy --workspace --all-targets` (x86_64) — clean
- `cargo clippy --workspace --all-targets --target aarch64-unknown-linux-gnu`
  (CC=clang, CFLAGS isystem /tmp/a64inc, AR=llvm-ar-18) — clean
- Generator cross-check: Python ref == standalone Rust ref == in-tree generator
- Boundary math: tail write `end_sector==capacity` is STATUS_OK; cluster 15 in-bounds
- `git status` — clean working tree

## Verdict

**APPROVE**

The change is correct, deterministic, fully tested on both arches, and the working
tree is clean. The suggestions below are improvements for the downstream asm consumer
and long-term maintainability, none blocking.

## Stats

- Files changed: 6 (+345 / -1)
- New files: `tests/nanokernel/src/image.rs` (176 LOC),
  `crates/dh-vmm/tests/blk_fixture.rs` (161 LOC)
- New deps: `blake3` (nanokernel dev/runtime), workspace `blake3 = "1"`
- Findings: 0 critical, 0 important, 4 suggestions, 1 needs-discussion
