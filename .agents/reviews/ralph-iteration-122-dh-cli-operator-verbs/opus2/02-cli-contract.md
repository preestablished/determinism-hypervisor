# CLI Contract

The top-level usage includes the new operator verbs at `tools/dh-cli/src/cli.rs:24`, and dispatch wires them to `crate::ops::dispatch` at `tools/dh-cli/src/cli.rs:35`.

JSON output for replay/verify is now line-oriented NDJSON:

- progress: `{"op":"verify","status":"progress","progress":...}`
- success: `{"op":"verify","status":"ok"}`
- late stream error: prior progress lines remain written, then dispatch emits a JSON error line

This is a reasonable operator contract for long-running streams.

Endpoint behavior is unchanged from the first review. Default endpoint is TCP `http://127.0.0.1:7400` at `tools/dh-cli/src/ops.rs:13`, bare values are normalized by prepending `http://` at `tools/dh-cli/src/ops.rs:564`, and connection uses `HypervisorWorkerClient::connect` at `tools/dh-cli/src/ops.rs:236`. The proto comment says `dh-workerd` serves TCP and UDS (`proto/hypervisor.proto:17`), but this CLI currently only supports TCP-style tonic URIs. That is acceptable for this iteration if deliberate, but should be made explicit or expanded later.

`replay` remains an alias for `VerifyReplay` with `bisect_on_divergence=false`. That is technically valid against the current proto surface, but the usage text still does not explain that `replay` is not a separate daemon RPC.

Manual JSON escaping still produces valid JSON. It remains byte-oriented, so non-ASCII diagnostic strings are not preserved as their original Unicode scalar values, but this is not a blocker for current gRPC/status output.
