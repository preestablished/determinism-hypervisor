# Review Overview

- **Branch:** `ralph/iteration-59-snapshot-store-readiness-verification`
- **Base:** `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Commits:** 1 (`6ab5924` — ralph: iteration 59 checkpoint - snapstore-client workspace dep + readiness gate)

## Summary

This change wires in a third sibling-repo path dependency — `snapstore-client` from
`../snapshot-store/crates/snapstore-client` — as the M4 store-integration readiness gate
(bead `4nj`, risk R12). It adds the dep to `[workspace.dependencies]`, consumes it as a
`dev-dependency` of `dh-snapshot`, and adds a compile-time surface-pin integration test
(`crates/dh-snapshot/tests/snapstore_readiness.rs`) that references the sibling crate's
gRPC client surface (`SnapstoreClient`, `blocking::SnapstoreClient`, `Transport`,
`ClientError`, and the `put_pages` signature) as function items so the workspace build
breaks loudly here — with a readable name — if the sibling renames or removes any of them.
Critically, the same commit also adds the `../snapshot-store` checkout step to all three CI
lanes that build the workspace (`ci.yaml` host + kvm-intel, `nightly-drift.yaml`
determinism-canary), landing the CI sibling checkout atomically with the dep — necessary
because cargo resolves path deps at `cargo-metadata` time. Docs (`test-partitioning.md`)
are updated to note the new `zstd-sys` C dependency in the aarch64 cross-build path.

## Verification performed

- Read the full non-lock diff and all six changed files.
- Validated every surface pin in the test against the real sibling crate
  (`client.rs`, `blocking.rs`, `transport.rs`, `lib.rs`): all methods, the `Transport`
  variants/fields, the re-exports, and the `put_pages` signature
  `(Vec<(u64, Vec<u8>)>) -> ClientResult<(u64, u64)>` match exactly.
- Cross-checked the test's surface claims against `.agents/docs/snapshot-store/API.md` §1.
- Spot-checked the `Cargo.lock` diff: added packages are the expected tonic/prost/axum/
  hyper/tokio/zstd gRPC transitive closure plus build tooling (`prost-build`,
  `protoc-bin-vendored`); removals are version-bump re-resolution of the shared closure.
  Nothing unrelated churns.
- Built and ran the gate locally: `cargo test -p dh-snapshot --test snapstore_readiness`
  compiles and passes (the compile is the real assertion).

## Verdict

**APPROVE**

The change is correct, minimal, follows the two established sibling-path-dep patterns
exactly, and the CI checkout lands atomically with the dep. The only findings are one
minor doc-comment inaccuracy in the test and a few non-blocking nits (see 01/02). None
block merge.

## Stats

- Files changed: 7 (6 reviewed in full + `Cargo.lock`)
- Lines (excluding `Cargo.lock`): +107 / −3
- `Cargo.lock`: +1558 / −96 (mechanical transitive-closure churn; spot-checked)
- Commits: 1
