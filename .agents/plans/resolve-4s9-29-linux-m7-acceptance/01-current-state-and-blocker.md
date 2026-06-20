# Current State And Blocker

## Bead State

`bd show determinism-hypervisor-4s9.29` reports:

- status: `BLOCKED`
- priority: `P0`
- title: `Add Linux M7 fork VerifyReplay acceptance and nightly canary`
- dependencies: `4s9.21`, `4s9.27`, and `4s9.28`, all closed

The bead notes say the current guard is intentional: `DH_M7_ACCEPT_GUEST=linux` must fail loudly until the M7 harness actually boots the M9 Linux fixture and proves Linux VerifyReplay.

## Current M7 Harness Shape

Relevant file: `crates/dh-worker/tests/m7_fork_verify.rs`.

The harness currently implements nanokernel acceptance only:

- creates a `pad_echo` nanokernel root VM;
- takes a root snapshot;
- forks children from the live root lease;
- injects a deterministic `PAD_SET` burst into each child;
- runs each child for `IcountBudget(RUN_BUDGET)`;
- takes a child snapshot with `seal_input_log = true`;
- fetches the child DHILOG by `input_log_id`;
- validates the child log as one lineage edge;
- verifies each child with `VerifyReplay`.

The Linux blocker is explicit:

```rust
fn reject_unimplemented_linux_m7_acceptance(test_name: &str) {
    if std::env::var(GUEST_ENV).as_deref() == Ok("linux") {
        panic!(...);
    }
}
```

Both ignored acceptance tests call this guard before building the nanokernel fixture:

- `m7_accept_1000_seeded_forks_verify_replay_all`
- `m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs`

There is also an ignored guard test:

- `linux_m7_acceptance_requires_real_linux_fixture`

Final implementation should remove or replace this guard. A Linux acceptance command must not pass by running zero tests, guard-only tests, or nanokernel fixture tests.

## Nanokernel-Specific Assumptions To Split Out

These existing assertions are correct for `pad_echo`, but wrong as shared Linux assertions:

1. `run_child` injects `PAD_SET` events and requires exactly `BURST_EVENTS` scheduled events.
2. `run_child` requires `run.icount == RUN_BUDGET` and `run.vns == VNS_PER_SECOND`.
3. `validate_single_edge_lineage` requires every canonical DHILOG record to be `RecordBody::PadSet`.
4. `verify_child` requires `VerifyReplay.Done.total_icount == RUN_BUDGET`.
5. The full acceptance asserts `unique_hashes.len() == jobs`, which depends on the nanokernel pad input script producing distinct child states.

For Linux, the harness must instead validate a frame-budget post-READY workload, frame marks, epoch hashes, and VerifyReplay end-state equality.

## Existing Linux Anchors

Relevant file: `crates/dh-worker/tests/common/mod.rs`.

The implementation should reuse the existing M9 Linux helper stack:

- `m9_artifacts`
- `populate_m9_image_cache`
- `m9_masked_cpuid_table`
- `m9_linux_machine_config`
- `m9_worker_config`
- `m9_linux_ready_snapshot_with_config`

`m9_linux_ready_snapshot_with_config` already:

- validates staged `DH_M9_*` artifacts;
- checks KVM dirty-ring support;
- populates the worker image cache;
- creates an M9 Linux worker service;
- boots Linux until the `Ready` SDK event;
- takes a READY snapshot with `seal_input_log = true`;
- returns the live lease, READY snapshot ref, READY state hash, worker service, and snapstore client.

However, it currently accepts only a slot count and maps it to cores `0..slots`. M7 acceptance must honor `DH_M7_ACCEPT_SLOT_CORES`, especially `2-5` on this host. Add an explicit-slot-core variant before using it for M7 Linux.

Relevant file: `crates/dh-worker/tests/m5_record_replay.rs`.

The Linux M5 corpus provides working constants and parsing patterns:

```rust
const M9_LINUX_CORPUS_FRAMES: u32 = 5;
const M9_LINUX_CORPUS_HARD_CAP: u64 = 5_000_000;
const M9_LINUX_CORPUS_EPOCH_LEN: u64 = 745_000;
```

It also contains a proven pattern for:

- fetching stored input logs from snapstore;
- parsing DHILOG with `LogReader`;
- requiring nonzero `EPOCH_HASH` records;
- comparing DHILOG header identity to live snapshot identity;
- counting `VerifyReplay` `EpochOk` messages;
- checking `VerifyReplay.Done.end_state_hash` against the post-run snapshot state hash.

## Current Nightly And Docs State

Relevant file: `.github/workflows/nightly-drift.yaml`.

The current `m7-fork-verify-100` job sets:

```yaml
DH_M7_ACCEPT_JOBS: 100
DH_M7_ACCEPT_SLOT_CORES: 2-5
DH_M7_ACCEPT_ALLOW_SKIP: "0"
```

It does not set `DH_M7_ACCEPT_GUEST=linux`, `DH_M9_ALLOW_SKIP=0`, or any `DH_M9_*` artifact paths.

Relevant file: `docs/ops/test-partitioning.md`.

The current M7 rows are nanokernel/default commands. The M9 Linux artifact section already documents the required `DH_M9_*` environment variables and the no-skip rule for final Linux gates.
