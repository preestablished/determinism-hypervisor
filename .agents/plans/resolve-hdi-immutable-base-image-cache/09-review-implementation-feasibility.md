# Implementation Feasibility Review

Reviewer: Pauli

## Findings

1. The original plan listed `08-review-security-correctness.md` and
   `09-review-implementation-feasibility.md`, and `07-review-resolution.md`
   blocked implementation until those files existed. They were absent. This
   review file and `08-review-security-correctness.md` now satisfy that gate.

2. The sparse oversized test did not prove failure before hashing. The plan now
   requires a helper-level small-limit sparse-file test and code-review
   confirmation that metadata length is checked before allocation/read/hash.

3. Base-image service error coverage was underspecified. The plan now requires
   explicit base-image status-code cases rather than accepting generic
   kernel/initramfs resolver mapping tests.

## Feasibility Notes

The owned-byte design is feasible in this repo without adding unsafe code:

- `dh-worker` keeps `#![forbid(unsafe_code)]`.
- `dh-vmm` can add byte-backed `FileBase` behavior with safe Rust.
- Existing file-backed `FileBase` fixture tests can remain.
- The worker resolver owns the cache seam, so `CreateVm`, `RestoreSnapshot`,
  `Fork`, and `VerifyReplay` should all inherit the fix through
  `open_base_image`.

The reviewer also compile-checked the focused targets:

```bash
cargo test -p dh-worker image_resolver --no-run
cargo test -p dh-vmm blkfile --no-run
```

Both passed before implementation.

## Non-Blocking Suggestions Applied

- `03-implementation-sequence.md` now spells the return as
  `Ok((path, FileBase::from_owned_bytes(out)))` or equivalent.
- `02-contract-decision.md` now calls out per-runtime memory cost and current
  reference-host feasibility.
