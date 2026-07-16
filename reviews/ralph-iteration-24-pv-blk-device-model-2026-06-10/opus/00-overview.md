# pv-blk Device Model — Review Overview

- **Branch:** `ralph/iteration-24-pv-blk-device-model` vs `main`
- **Bead:** determinism-hypervisor-sai
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Commit under review:** `32b6cca` (ralph: iteration 24 checkpoint — pv-blk device model: RO base + CoW overlay)

## Scope

New simplified virtio-blk device per ARCHITECTURE §6.5: a one-deep request
register set with synchronous completion, a read-only base image behind the
`BlockBase` seam, and a copy-on-write overlay of 64 KiB clusters populated by
read-modify-write on first write. Snapshot/restore serializes dirty clusters
sorted by index with a per-cluster blake3 digest.

Files:
- `crates/dh-devices/src/blk.rs` (+624) — device model, `BlockBase` trait,
  `PvBlk`, `DetDevice` impl, 9 unit tests.
- `crates/dh-vmm/src/blkfile.rs` (+186) — production `FileBase` (`O_RDONLY` +
  `read_exact_at`), 2 tests including the §6.5 mtime/bytes acceptance.
- `crates/dh-devices/src/lib.rs` (+1) — `pub mod blk;`
- `crates/dh-vmm/src/lib.rs` (+1) — `pub mod blkfile;`

## Verification performed

- `cargo test -p dh-devices blk` → 9 passed.
- `cargo test -p dh-vmm blkfile` → 2 passed.
- `cargo test -p dh-devices no_host_ambient_authority` (deny-list grep gate) → passed.
- `cargo clippy -p dh-devices -p dh-vmm` → clean (no warnings; `disallowed_types`/
  `disallowed_methods` are `deny` in dh-devices).
- Cross-checked register layout, widths, CMD/STATUS codes, and CoW semantics
  against ARCHITECTURE.md §6.1 and §6.5.

## Summary

This is a clean, well-documented, well-tested device model that conforms to
§6.5 on every register, width, and behavior I checked. Arithmetic overflow
paths are correctly guarded (the one apparent hazard — `sector * 512` — is
provably bounded by the prior `end_sector <= capacity_sectors` check; the
`gpa += take` advancement cannot overflow because each advance follows a
successful in-range guest-memory access). The deny-list discipline is honored:
no host I/O tokens in `blk.rs`, and the production file backend correctly lives
in dh-vmm behind the `BlockBase` seam. Snapshot determinism (sorted clusters,
order-free serialization) is directly tested with two devices dirtying clusters
in opposite order.

The findings are not correctness bugs in the happy path; they are about
**replay-safety reasoning that the code relies on but does not fully enforce or
document**, plus a few hardening and test-coverage gaps. The single most
important item is the partial-completion semantics on `STATUS_MEM_FAULT`: the
device leaves guest RAM and overlay state partially mutated, which is fine for
determinism *only because* the inputs are identical on replay — but nothing in
the code or the run-control contract is shown to guarantee the faulting request
replays bit-identically. This deserves an explicit argument and ideally a test.
See 01 for details.

No issue rises to a blocking defect. I recommend addressing the Important items
(partial-completion documentation + a replay/idempotence test, and the
host-io-errors contract being a documented convention rather than an enforced
one) before merge, but they are small.

## Verdict

**APPROVE** (with Important follow-ups — none blocking, all small)

## Stats

| Category | Count |
|---|---|
| Critical | 0 |
| Important | 3 |
| Suggestions | 6 |
| Positive notes | 7 |
| Tests added | 11 (9 in blk.rs, 2 in blkfile.rs) |
| Net LOC | +812 |

> Note: the change description states "14 new tests"; the branch actually adds
> 11 (`grep -c '#\[test\]'`: 9 + 2). Not a defect — a miscount in the summary.
