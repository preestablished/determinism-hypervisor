# Correctness

No blocking correctness issues found on re-review.

RPC request construction:
- `tools/dh-cli/src/ops.rs:162-167` sends `TakeSnapshotRequest` with the parsed lease, `seal_input_log`, and no capture.
- `tools/dh-cli/src/ops.rs:177-181` sends `RestoreSnapshotRequest` with a `SnapshotRef` and optional entropy seed bytes.
- `tools/dh-cli/src/ops.rs:192-197` sends `ForkRequest` with parent lease, count, and repeated entropy seeds.
- `tools/dh-cli/src/ops.rs:257-262` sends `VerifyReplayRequest` with the base snapshot, selected log source, and selected bisection flag.

Request semantics:
- Snapshot still defaults `seal_input_log=true` at `tools/dh-cli/src/ops.rs:376` and only flips false for `--no-seal-input-log` at `tools/dh-cli/src/ops.rs:385`. The fake worker test pins both request bodies at `tools/dh-cli/src/ops.rs:1232-1268`.
- Replay still forces `bisect_on_divergence=false` at `tools/dh-cli/src/ops.rs:203-204`, while verify uses the parser-selected value at `tools/dh-cli/src/ops.rs:206-220`. The fake worker test pins replay false and verify default true at `tools/dh-cli/src/ops.rs:1320-1378`.
- Restore and fork wire fields are pinned by `tools/dh-cli/src/ops.rs:1270-1318`.

Streaming behavior:
- `tools/dh-cli/src/ops.rs:267-282` writes and flushes each progress message before awaiting the next stream item.
- `tools/dh-cli/src/ops.rs:284-294` writes a final `ok` marker after a clean stream.
- Late stream errors return `OpError::Rpc` after already-written progress. `dispatch` then renders the final error object for JSON mode via `tools/dh-cli/src/ops.rs:132-145`.
- The fake worker covers progress-before-late-error at `tools/dh-cli/src/ops.rs:1380-1408`.

JSON behavior:
- Progress JSON mode now emits one complete JSON object per line at `tools/dh-cli/src/ops.rs:270-276`, avoiding the earlier all-progress array buffering issue.
- Parse errors with `--json` now produce a usage error JSON object at `tools/dh-cli/src/ops.rs:103-119`.
- String-bearing fields continue to pass through `json_escape` at `tools/dh-cli/src/ops.rs:812-825`.

Boundary check:
- `tools/dh-cli/Cargo.toml:10`, `tools/dh-cli/Cargo.toml:20-21`, and `tools/dh-cli/Cargo.toml:28` add only generated API/runtime/test transport dependencies.
- No reviewed file adds a `dh-worker` dependency or imports `dh_worker`.
