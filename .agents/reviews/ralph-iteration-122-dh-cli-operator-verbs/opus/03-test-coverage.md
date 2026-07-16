# Test Coverage

Coverage is substantially improved.

Current focused test run:

```bash
cargo test -p dh-cli
```

Result: passed. `dh-cli` unit tests: 13 passed. Existing `boot_hello` and `skid_gate` integration tests also passed.

Coverage present:
- Parser tests cover snapshot defaults, fork seed counts, verify log IDs, replay rejecting bisection flags, flag-like missing values, and conflicting verify bisection flags.
- `snapshot_rpc_sends_seal_input_log_default_and_override` uses an in-process generated tonic client/server path to pin `TakeSnapshotRequest.seal_input_log` true by default and false with `--no-seal-input-log`.
- `restore_and_fork_rpc_fields_are_pinned` pins restore snapshot/seed fields and fork parent/count/entropy seed fields.
- `replay_and_verify_rpc_fields_and_streaming_output_are_pinned` pins replay/verify `VerifyReplayRequest` fields and bisection defaults.
- `verify_stream_preserves_progress_before_late_error` verifies that progress written before a later stream error is preserved.

Remaining non-blocking coverage gaps:
- The late-error test exercises `execute_to_writer`, not the full `dispatch` path that prints the final JSON error object after a stream error.
- JSON output is still mostly tested by exact strings or substring checks rather than parsing each emitted line with a JSON parser.
- There is no CLI subprocess/integration test for the top-level `--json` parse-error object, though the implementation is straightforward and visible in `dispatch`.
