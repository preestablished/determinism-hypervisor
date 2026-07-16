# Correctness

The implementation still satisfies the dependency intent: `dh-cli` depends on `dh-proto`, `tokio`, and `tonic`, but not `dh-worker`. `Cargo.lock` shows only those new edges for `dh-cli`.

Request construction remains aligned with the generated proto client:

- Snapshot sends `TakeSnapshotRequest { lease, seal_input_log, capture: None }` at `tools/dh-cli/src/ops.rs:162`. The parser still defaults `seal_input_log` to `true` at `tools/dh-cli/src/ops.rs:376` and only flips it for `--no-seal-input-log` at `tools/dh-cli/src/ops.rs:385`.
- Restore sends `SnapshotRef` and optional `entropy_seed` at `tools/dh-cli/src/ops.rs:177`. Empty seed continues to represent "continue snapshot PRNG stream".
- Fork sends `parent`, `count`, and `entropy_seeds` at `tools/dh-cli/src/ops.rs:192`; parser validation rejects zero count and partial explicit seed lists at `tools/dh-cli/src/ops.rs:463`.
- Replay and verify both send `VerifyReplayRequest` at `tools/dh-cli/src/ops.rs:257`, with replay forcing `bisect_on_divergence=false` at `tools/dh-cli/src/ops.rs:203` and verify using the parsed flag at `tools/dh-cli/src/ops.rs:206`.

The previous streaming correctness issue is fixed. `stream_verify_like_output` writes each progress message as it arrives and flushes after every line at `tools/dh-cli/src/ops.rs:267` through `tools/dh-cli/src/ops.rs:282`. If a later `stream.message()` returns a gRPC error, already-written progress is preserved before the error propagates.

Value parsing was also hardened. Flags that require values now reject missing or flag-looking values at `tools/dh-cli/src/ops.rs:553`, and conflicting verify bisection flags are rejected at `tools/dh-cli/src/ops.rs:523`.

I did not find a remaining correctness blocker.
