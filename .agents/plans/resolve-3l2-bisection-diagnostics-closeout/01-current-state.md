# Current State

## Bead State

`bd show determinism-hypervisor-3l2` currently reports:

- Status: `BLOCKED`
- Priority: `P1`
- Owner/assignee: Matt Spurlin
- Type: `feature`
- Label: `impl`

The parent notes say the original blocker was the absence of a trustworthy
expected-state source for midpoint diagnostics. The notes then say the local
unblock path was split into seven child beads, and the dependency graph now has
`3l2` depending on `3l2.1` through `3l2.7`.

All seven dependency beads are now closed.

## Closed Child Bead Evidence

The child close reasons say the following landed:

- DHILOG `BISECTION_CHECKPOINT` AUX kind `0x46` codec, writer emission, reader
  parsing, inspection, validation, and layout tests.
- Non-mutating full checkpoint snapshot capture in `dh-worker` snapshot engine.
- Recorder checkpoint scheduling/AUX emission with safe checkpointability gates
  and epoch ordering.
- VerifyReplay bisection checkpoint indexing, selection, and validation.
- Snapshot comparison utilities producing RIP/register/page diagnostics.
- Replay probe capture and `BisectionDivergence` construction.
- End-to-end service/CLI tests for checkpointed and checkpoint-less behavior.

The parent stayed `BLOCKED`, probably because no final parent-level audit and
closeout happened after `3l2.7`.

## Code Evidence To Inspect

Primary files:

- `proto/hypervisor.proto`
- `crates/dh-inputlog/src/dhilog.rs`
- `crates/dh-inputlog/src/reader.rs`
- `crates/dh-worker/src/bisection_index.rs`
- `crates/dh-worker/src/snapshot_compare.rs`
- `crates/dh-worker/src/snapshot_engine.rs`
- `crates/dh-worker/src/replay_engine.rs`
- `crates/dh-worker/src/verify_replay.rs`
- `crates/dh-worker/src/service.rs`
- `tools/dh-cli/src/ops.rs`

Important current symbols to verify:

- `KIND_BISECTION_CHECKPOINT`
- `LogWriter::bisection_checkpoint`
- `RecordBody::BisectionCheckpoint`
- `BisectionCheckpointIndex`
- `compare_snapshots`
- `ReplayError::BisectionDivergence`
- `verify_replay_with_bisection_progress`
- `verify_progress_to_proto`
- `VerifyReplayRequest.bisect_on_divergence`

## Tests Already Present

Current test names found in the repository include:

- `verify_replay_rpc_streams_divergence_for_semantically_bad_log`
- `verify_replay_rpc_streams_bisection_divergence_with_checkpoint_evidence`
- `verify_replay_rpc_rejects_invalid_bisection_checkpoint_gap`
- `verify_replay_divergence_mapping_is_honest_about_bisection`
- `lapc_verify_replay_bisection_reports_lapic_reg_diff_on_mutation`
- `verify_renders_bisection_divergence_json_and_human`

The implementation agent must not assume these names are sufficient. It must
run them and inspect what they assert against the parent requirements.

## Reference Host Context

This workspace is on the Linux/KVM reference host. Do not downgrade acceptance
to non-KVM compile checks unless KVM or host prerequisites genuinely fail. If a
host prerequisite fails, leave `3l2` open with the exact blocker and command
output.
