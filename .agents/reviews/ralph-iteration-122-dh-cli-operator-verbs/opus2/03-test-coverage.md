# Test Coverage

Focused gate run:

```bash
cargo test -p dh-cli -- --nocapture
```

Result: 13 unit tests, 8 integration tests, and doc tests passed.

The previous no-live-worker gap is closed. `tools/dh-cli/src/ops.rs:1232` through `tools/dh-cli/src/ops.rs:1408` now spins up a generated `HypervisorWorkerServer` over a local TCP listener and drives the generated client through `execute_to_writer`.

The new boundary tests pin:

- `TakeSnapshotRequest.seal_input_log` default and `--no-seal-input-log` override.
- Restore snapshot hash and entropy seed.
- Fork parent lease, count, and explicit seed list.
- Replay and verify `VerifyReplayRequest` base, log oneof, and bisection flag.
- Progress output before a late stream `DATA_LOSS` error.

Parser coverage also now includes flag-looking missing values and conflicting verify bisection flags.

Remaining non-blocking coverage gaps:

- Inline `--input-log PATH` is parsed, but the fake-worker boundary tests only exercise `--input-log-id`.
- Dispatch-level JSON error emission after a late stream error is not directly process-tested because `dispatch` exits the process; the writer-level behavior is covered.
- No test covers UDS endpoints, matching the current TCP-only implementation.
