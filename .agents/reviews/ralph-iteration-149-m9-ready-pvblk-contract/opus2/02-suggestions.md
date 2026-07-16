- `crates/dh-worker/src/proto_map.rs:234`: `machine_config_to_proto` panics on a non-canonical BzImage cmdline via `expect`. Consider validating up front or returning a `Result` so invalid in-memory configs cannot crash a caller during wire conversion.

- `tests/determinism/tests/linux_boot_trace.rs:35`: The ignored Linux smoke only checks programmed registers and copied boot_params; it never runs the vCPU. Add a minimal run/exit assertion with real artifacts so the test can catch entry/load-layout mistakes.
