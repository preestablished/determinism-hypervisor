# Review Overview — proto-seam decision + protobuf codegen skeleton

- **Branch:** `ralph/iteration-60-proto-seam-decision-real-protobuf-codegen` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Commit under review:** `65a18c3` (1 commit; "ralph: iteration 60 checkpoint - proto seam decision + codegen skeleton")
- **Bead:** determinism-hypervisor-v8p (decision); follow-up bead bcb (full §2 surface), backlog bead 0ic (control-plane adoption)

## Summary

This change resolves the decision bead v8p — where real protobuf codegen for the
`determinism.hypervisor.v1` gRPC surface should live — in favor of **this repo**,
mirroring the sibling snapshot-store precedent (`snapstore-client` generates its
own proto in-repo with `protoc-bin-vendored` and a documented re-export seam for a
later control-plane adoption). The diff lands four things: (1) a decision doc
`docs/decisions/proto-seam.md`; (2) promotion of `proto/hypervisor.proto` from an
empty placeholder service to a real schema skeleton (§2.1 core types + §2.8
`GetWorkerInfo` leg + `service HypervisorWorker` with one rpc); (3) `dh-proto`
codegen via `build.rs` (tonic-build + vendored protoc) plus a `v1` module behind a
single `include_proto!` re-export seam, with the `determinism-proto` facade
narrowed to `features=["common"]` and a prost round-trip + client/server stub-pin
test; and (4) workspace `tonic`/`prost`/`tonic-build`/`protoc-bin-vendored` deps at
the same versions as `../snapshot-store`. I verified the skeleton's message and
field-number text **byte-for-byte against the normative API.md §2.1 and §2.8** —
they match exactly, so bead bcb's fill-in is purely additive (no field renumber).
The `dh-proto` test passes, the full workspace builds clean, and there are no
external consumers of the removed `determinism_proto::hypervisor` placeholder.

## Verdict

**APPROVE**

The skeleton is faithful to the normative schema, the codegen is live and tested,
the decision mirrors an established sibling precedent, and the re-export seam is
kept genuinely single-point. Findings below are all non-blocking (one Important
observation about a missing build-script loud-failure guard that the research file
flags, plus minor suggestions). Nothing blocks merge.

## Stats

| Metric | Value |
|---|---|
| Files changed | 7 (incl. Cargo.lock, +4 lines, excluded from diff text) |
| New files | 3 (`build.rs`, `proto-seam.md`, plus proto rewrite) |
| Lines added / removed | +216 / −6 |
| Critical issues | 0 |
| Important issues | 1 (non-blocking) |
| Suggestions | 4 |
| Tests | `dh-proto` 1 test passes; full workspace `cargo build` clean |
| Schema fidelity | §2.1 + §2.8 match API.md byte-for-byte |
