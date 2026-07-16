# Action Items

## Critical

- None

## Important

- Fix `tests/determinism/tests/linux_boot_trace.rs:313` so detchannel and serial `VcpuExit::IoIn` buffers are not blindly filled with zero before classification. Either answer those ports with the real deterministic models before re-entering KVM, or terminate the trace with an explicit `terminal_reason` when such an input requires a model the trace loop does not provide.

- Fix `tests/determinism/tests/linux_boot_trace.rs:440` so `DH_M9_TRACE_BOOT=1` does not implicitly require a working pinned perf instruction counter unless the operator explicitly requested `DH_M9_TRACE_ICOUNT_LIMIT`. If instruction-count limiting remains the default, catch counter setup failures and emit a trace artifact with a clear terminal reason instead of panicking before the artifact exists.

## Suggestions

- Change `tests/determinism/tests/linux_boot_trace.rs:90` so the fixed trace artifact path is written only for explicit trace mode, or otherwise mark one-exit smoke artifacts so they cannot be confused with full traces.

- Consider replacing the manual JSON builder at `tests/determinism/tests/linux_boot_trace.rs:476` with `serde_json` construction and update the unit test to parse the output rather than relying on substring checks.

- Tighten `tests/determinism/tests/linux_boot_trace.rs:423` environment parsing so common values such as `true` are accepted or invalid non-empty values produce a clear panic.
