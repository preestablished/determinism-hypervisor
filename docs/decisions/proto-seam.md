# Decision: hypervisor proto codegen lives in this repo (dh-proto)

**Bead:** determinism-hypervisor-v8p · **Status:** decided 2026-06-11 ·
**Owner mechanism:** `proto/hypervisor.proto` + `crates/dh-proto/build.rs`

## Context

The API.md §2 gRPC surface (`determinism.hypervisor.v1`, served by
dh-workerd) needs real tonic/prost codegen before any M6 implementation
bead starts. Two candidate homes existed:

1. **control-plane's `determinism-proto`** — the cross-repo contract facade
   this workspace already path-depends on. Today it is hand-written
   placeholder structs behind per-domain feature gates, with NO codegen
   machinery; its own doc says "M0 keeps code generation intentionally
   thin." Its `hypervisor::v1` module had two placeholder structs
   (`SnapshotRef`, `Lease`).
2. **this repo** — promote `proto/hypervisor.proto` (previously an empty
   placeholder service) and generate in `dh-proto`.

## Decision

**Codegen lives in this repo.** `proto/hypervisor.proto` is the canonical
schema; `dh-proto`'s build.rs compiles it with tonic-build and a vendored
protoc (`protoc-bin-vendored`), exposing the result as `dh_proto::v1`.
`dh-proto` keeps re-exporting `determinism_proto::common` as the cross-repo
facade for non-hypervisor shared types; determinism-proto's placeholder
`hypervisor` feature is no longer consumed by this workspace.

Rationale:

- **Sibling precedent.** snapshot-store faced the same seam and solved it
  the same way: `snapstore-client` generates `determinism.snapstore.v1`
  in-repo from its own `proto/snapshot_store.proto`, with a documented
  single-module re-export seam for a future control-plane adoption
  ("adopt-snapstore-proto-v1"). Diverging from that pattern would give the
  two service repos two different contract mechanics for no benefit.
- **Iteration locality.** The hypervisor schema will iterate with every M6
  bead. Codegen in determinism-proto would make every schema change a
  cross-repo commit pair with HEAD-coupled path deps (no rev pinning) —
  maximum exposure to skew, for a contract only this repo serves.
- **No build-machinery export.** Putting tonic/prost/build.rs into
  determinism-proto would force its codegen toolchain (and protoc story)
  onto all nine downstream feature domains, most of which are still
  hand-written placeholders by design.
- **Runner provisioning is a no-op.** `protoc-bin-vendored` ships the
  protoc binary with the build; neither CI lane nor the kvm-intel box needs
  protoc installed (noted on bead py3, which had listed protoc as a
  candidate provisioning item).

## The re-export seam (kept open)

`dh_proto::v1` is a single module whose body is
`tonic::include_proto!("determinism.hypervisor.v1")`. If control-plane
later adopts hypervisor codegen (an "adopt-hypervisor-proto-v1" request,
mirroring snapstore's), the module body swaps to a re-export of the
published crate and no other workspace code changes. Tracked as a backlog
bead; not a precondition for anything in M4–M7.

## Skeleton scope (this bead) vs full surface (bead bcb)

The committed skeleton proves the seam end-to-end on x86_64 and aarch64:
§2.1 core types (`SnapshotRef`, `StateHash`, `Lease`), the §2.8
`GetWorkerInfo` leg, and `service HypervisorWorker` with client+server
stubs, pinned by a prost round-trip test in `dh-proto`. Bead bcb fills in
the rest of the API.md §2 surface (slot lifecycle, execution, introspection,
verification, watch streams, error model).

## Consequences

- determinism-proto's `hypervisor` feature becomes dead from this repo's
  perspective; the placeholder structs there should not be extended. The
  cross-repo doc issue is control-plane's to action at adoption time.
- `cargo metadata` still requires all three sibling checkouts (README
  "Sibling repos"); this decision adds no new sibling coupling.
- build.rs sets `PROTOC` to the vendored path at build time, so builds are
  hermetic with respect to any system protoc.
