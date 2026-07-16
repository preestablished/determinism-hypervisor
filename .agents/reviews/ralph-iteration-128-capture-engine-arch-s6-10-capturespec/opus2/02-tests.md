# Tests

Ran:

```text
cargo test -p dh-worker run_capture_spec_reads_manifest_ranges_and_lz4_framebuffer -- --nocapture
cargo test -p dh-worker take_snapshot_capture_checks_layout_version_and_returns_features -- --nocapture
cargo test -p dh-worker --test replay_engine replay_reproduces_the_recording_bit_identically -- --nocapture
```

Results:

- `run_capture_spec_reads_manifest_ranges_and_lz4_framebuffer`: passed.
- `take_snapshot_capture_checks_layout_version_and_returns_features`: passed.
- `replay_reproduces_the_recording_bit_identically`: passed.

Coverage gaps that matter:

- No test records a DetChannel-enabled guest through the new `service_exit_with_detchannel` path and then verifies the sealed log via `VerifyReplay`.
- No capture-neutrality test compares capture vs. no-capture state hashes / snapshot refs for the same base and inputs.
- No test covers oversized `CaptureSpec` output or a guest manifest with an oversized framebuffer region.
- No test covers `Run` with an invalid post-run `CaptureSpec` and asserts what happens to slot position and log state.
