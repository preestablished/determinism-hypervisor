# CLI Contract

The CLI contract is in good shape for this bead.

Resolved from the first review:
- Parse errors now honor `--json` at `tools/dh-cli/src/ops.rs:103-119`.
- Value-taking flags now reject a following flag-like token at `tools/dh-cli/src/ops.rs:553-561`.
- `verify` now rejects conflicting `--bisect` and `--no-bisect` at `tools/dh-cli/src/ops.rs:523-534`.
- Replay/verify no longer buffer all progress until stream completion; each progress line is emitted immediately and flushed.

Usability notes:
- Top-level dispatch exposes all new verbs at `tools/dh-cli/src/cli.rs:35-39`.
- Endpoint normalization still accepts bare `host:port` and explicit URI schemes at `tools/dh-cli/src/ops.rs:564-570`.
- Snapshot defaults to sealed DHILOG capture with an explicit `--no-seal-input-log` escape hatch.
- Replay and verify intentionally share the worker RPC but expose different CLI defaults: replay is non-bisecting, verify defaults to bisection.

Residual non-blocking contract notes:
- Flag-like values are rejected globally for value-taking flags. That improves typo handling, but it means an input-log path literally beginning with `--` cannot be passed unless a future `--` sentinel is added.
- The proto comments advertise both TCP and `/run/dh/grpc.sock`; this CLI still supports ordinary tonic URI endpoints only.
- Seed length is enforced by parser errors, but the short usage text still says only `HEX`, not `32-byte HEX`.
