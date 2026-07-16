# Review: proto-seam decision + real protobuf codegen (2nd reviewer)

- **Branch:** `ralph/iteration-60-proto-seam-decision-real-protobuf-codegen`
- **Base:** `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Commit:** `65a18c3` ralph: iteration 60 checkpoint - proto seam decision + codegen skeleton

## Summary

This change resolves decision bead v8p — where the `determinism.hypervisor.v1`
gRPC contract's protobuf codegen lives — in favor of THIS repo, mirroring the
sibling snapshot-store's `snapstore-client` precedent (in-repo tonic-build +
vendored protoc, with a documented single-module re-export seam for a future
control-plane adoption). The skeleton `proto/hypervisor.proto` defines the §2.1
core common types (`SnapshotRef`, `StateHash`, `Lease`), the §2.8 `GetWorkerInfo`
leg (`GetWorkerInfoRequest`/`Response`, `DeterminismClass`), and a one-rpc
`service HypervisorWorker`. `dh-proto`'s new `build.rs` compiles it, `lib.rs`
exposes `dh_proto::v1` via `include_proto!`, narrows the determinism-proto facade
from `features=["hypervisor"]` to `["common"]`, and keeps re-exporting `common` +
`PROTO_VERSION`. A round-trip test pins the skeleton end-to-end. I verified every
skeleton field number character-by-character against the normative API.md §2.1/§2.8
text — **all match exactly**, so bead bcb (full §2 surface) remains an additive
fill-in, not a wire-breaking renumber. The build compiles clean across the
workspace, the test passes, and the determinism-proto narrowing is genuinely
complete (full feature-tree confirms only `common` is enabled anywhere).

## Verdict

**APPROVE**

This is a clean, well-documented, precedent-faithful skeleton. No Critical or
Important issues. A small number of non-blocking suggestions only.

## Stats

| Metric | Value |
|---|---|
| Files changed | 7 (+216 / −6) |
| New files | `crates/dh-proto/build.rs`, `docs/decisions/proto-seam.md` |
| Critical issues | 0 |
| Important issues | 0 |
| Suggestions | 4 |
| Build (`cargo build --workspace`) | PASS |
| Test (`cargo test -p dh-proto`) | PASS (1 test) |
| Field-number fidelity vs API.md §2.1/§2.8 | EXACT MATCH |
