# Review: proto schema v1 — full API.md §2 surface (bead bcb)

- **Branch:** `ralph/iteration-63-proto-schema-v1-full-api-surface`
- **Base:** `main` (HEAD `8a22a56`)
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Diff:** `/tmp/ralph63-diff.txt`

## Summary

This change fills `proto/hypervisor.proto` from the iteration-60 §2.1/§2.8 skeleton
out to the full API.md §2 surface: all 17 rpcs, every request/response message, the
five top-level enums (`HashEpochs`, `PixelFormat`, `StopReason`, `SlotState`,
`QuiesceMode`) plus the nested `MemPredicate.Op`, the §2.9 `ErrorDetail`, and the
§2.10 Quiesce leg. The fill-in is purely additive over the skeleton (no field
renumbering). Two test functions are added to `crates/dh-proto/src/lib.rs`: a
compile-time rpc-surface pin (`_all_seventeen_rpcs` referenced by
`all_seventeen_rpcs_are_generated`) and a message-shape/round-trip pin
(`full_surface_message_shapes`). API.md §2.8 is edited locally to rename
`SlotState.PAUSED → PAUSED_S` with an inline editorial note.

I independently verified the things that are easy to overlook:

- **Built and tested** `dh-proto` on x86_64: 3 tests pass, clippy clean (no warnings).
- **Cross-arch:** the aarch64 codegen output exists and is byte-identical in surface
  (2626 lines, same as x86_64) — `protoc_bin_vendored` covers aarch64.
- **Read the generated Rust** in `target/debug/build/dh-proto-*/out/determinism.hypervisor.v1.rs`
  for all the prost-mangling risk points (oneofs referencing same-named messages,
  nested enums, proto3 optional, `GuestEvent` dual-use, `RunResponse` embedded in a
  oneof). All generate unambiguous, compiling code.
- **Audited every enum value name across the whole package** for latent C++-scoping
  collisions (the rule that forced `PAUSED_S`): 28 distinct value names, **zero
  collisions** today.
- **Field-for-field diffed** 14 messages/enums against the normative API.md §2 text:
  all match. `ErrorDetail` is the one intentional deviation (API.md gives no field
  numbers; pinned in-proto), and the proto comment documents this accurately.
- **Cargo.lock:** unchanged (no new deps) — verified.
- **`sample_lease` / `Lease`:** unchanged; consumers are dh-proto-internal only, no drift.

## Verdict

**APPROVE.** No Critical or Important findings. The transcription is accurate, the
collision is handled correctly and documented, generated code is clean on both arches,
and the tests pin the load-bearing contract surface. The suggestions below are
non-blocking forward-guarding (a documented naming convention for future enum authors,
and a couple of test-pin completeness gaps).

## Stats

| Metric | Value |
|---|---|
| Files changed | 3 |
| Lines added / removed | +560 / −16 |
| RPCs defined | 17 |
| Top-level + nested enums | 5 + 1 |
| Generated Rust (x86_64 = aarch64) | 2626 lines / ~112 KB |
| dh-proto tests | 3 pass |
| clippy warnings | 0 |
| Enum value collisions (package-wide) | 0 |
| New deps (Cargo.lock delta) | 0 |
| Critical / Important / Suggestions | 0 / 0 / 4 |
