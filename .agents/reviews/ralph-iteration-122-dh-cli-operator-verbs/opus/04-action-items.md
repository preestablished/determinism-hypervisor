# Action Items

## Required

None.

## Recommended

- `tools/dh-cli/src/ops.rs:132-145` and `tools/dh-cli/src/ops.rs:1380-1408`: Add a full dispatch/subprocess-style test for JSON replay/verify late stream errors. The current test proves progress is written before the error from `execute_to_writer`; it does not capture the dispatcher's final JSON error line.

- `tools/dh-cli/src/ops.rs:296-360`, `tools/dh-cli/src/ops.rs:630-710`, and `tools/dh-cli/src/ops.rs:270-285`: Parse representative JSON output lines in tests. The current strings look valid, but a parser check would guard future hand-built JSON changes.

## Optional

- `tools/dh-cli/src/ops.rs:553-561`: Consider adding `--` sentinel support later if operators need input-log paths that literally begin with `--`.

- `tools/dh-cli/src/cli.rs:24`, `tools/dh-cli/src/ops.rs:615`, and `tools/dh-cli/src/ops.rs:618`: Document entropy seeds as 32-byte hex in usage text.

- `tools/dh-cli/src/ops.rs:236-239`: Consider UDS endpoint support if `/run/dh/grpc.sock` is intended to be a first-class operator path.
