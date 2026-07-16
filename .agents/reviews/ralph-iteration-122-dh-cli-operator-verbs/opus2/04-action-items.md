# Action Items

## Required

None. The previous Required findings are addressed.

## Recommended

- `tools/dh-cli/src/ops.rs:236`: Clarify or expand endpoint support. The worker contract includes TCP and UDS (`proto/hypervisor.proto:17`), but `normalize_endpoint` at `tools/dh-cli/src/ops.rs:564` treats every scheme-less value as TCP HTTP and cannot connect to `/run/dh/grpc.sock`. Either support UDS explicitly or make help/errors say this CLI currently accepts TCP tonic URIs only.

- `tools/dh-cli/src/ops.rs:203`: Clarify `replay` as `VerifyReplay` without bisection. This is defensible because the proto exposes `VerifyReplay`, not a separate `Replay` RPC, but the usage text at `tools/dh-cli/src/cli.rs:24` and `tools/dh-cli/src/ops.rs:620` still makes `replay` sound like a distinct execution verb.

- `tools/dh-cli/src/ops.rs:236`: Consider connect/RPC timeouts for operator commands. A daemon that accepts a connection but never responds can currently make the CLI wait indefinitely.

## Optional

- `tools/dh-cli/src/ops.rs:585`: Accept uppercase `0X` prefixes in `parse_hex_exact` if operator input should follow common hex conventions.

- `tools/dh-cli/src/ops.rs:553`: The new flag-looking-value rejection means `--input-log --some-file` is not accepted as a path. That is usually fine for operator CLI ergonomics, but it is worth documenting if strict POSIX-style path flexibility matters.

- `tools/dh-cli/src/ops.rs:812`: Consider using a structured JSON writer or char-based string escaping if preserving non-ASCII diagnostic text matters. The current byte-wise escape output is valid JSON but not a faithful Unicode string round trip.

- `tools/dh-cli/Cargo.toml:10`: If keeping non-x86 `dh-cli` builds minimal is important, move `dh-proto` under the same x86_64 target dependency group as `tokio` and `tonic`, since `ops` is cfg-gated.
