# ROM Bridge o73 READY Snapshot Handoff Plan

## Source Request

Implement the request at:

```text
/home/infra-admin/.agents/projects/determinism-hypervisor/requests/rom-bridge-o73-ready-snapshot-handoff/
```

The downstream blocker is `rom-operator-bridge-o73`: the bridge needs a private,
operator-approved `BRIDGE_REAL_SNAPSHOT_REF` that restores from a durable
snapstore data root through a snapstore-enabled worker.

## Key Constraints

Keep private values out of committed files, bead notes, PR bodies, and shared
terminal output. Private values include snapshot refs, lease tokens, worker or
snapstore socket paths, snapstore data roots, credentials, private bridge roots,
raw worker errors, and raw snapstore errors.

Do not satisfy this request with the existing ignored tests alone. The tests are
good implementation references, but they use `TempDir` snapstore state and do
not produce a durable bridge handoff.

Preserve the workspace architecture rule that nothing depends on `dh-worker`.
Do not add a `dh-worker` dependency to `tools/dh-cli` or any other workspace
member. Put the operator generator inside the `dh-worker` package, or implement
a runbook that drives `dh-workerd` over gRPC.

## Recommended Shape

Add an operator-only `dh-worker` binary named `dh-m9-ready-handoff` plus an ops
runbook. The binary should create and verify the durable READY snapshot; the
runbook should explain how to serve that data root later with `snapstore-server`
and `dh-workerd` for the bridge.

This keeps the implementation near:

- `crates/dh-worker/tests/common/mod.rs::m9_linux_ready_snapshot_with_slot_cores_and_config`
- `crates/dh-worker/tests/linux_worker_api.rs::run_pvblk_dev_vdb`
- `crates/dh-worker/src/bin/dh-workerd.rs`
- `crates/dh-worker/src/service.rs`
- `docs/ops/test-partitioning.md`

## Required Deliverables

Implementing agent should produce:

1. A durable private snapstore data root containing the M9/reference-workload
   READY snapshot.
2. A private env handoff file, mode `0600` or stricter, containing the bridge
   inputs required by `rom-operator-bridge`.
3. A sanitized public summary containing only booleans, counts, command shapes,
   and non-sensitive status.
4. Operator docs for regenerating, verifying, and serving the snapshot.
5. Tests that prove the generator preserves the privacy contract and destroys
   every lease it creates.

## Non-Goals

Do not make `rom-operator-bridge` generate snapshots.

Do not invent bridge workload or capture refs from public repo metadata. Accept
`BRIDGE_WORKLOAD_IMAGE_REF` and `BRIDGE_CAPTURE_SPEC_REF` as explicit
operator-approved private inputs, then write them to the private handoff file.

Do not print the real snapshot ref as a success message. It belongs only in the
private handoff file.
