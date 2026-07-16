# Action Items

## Critical

- None.

## Important

1. Restore the smoke-test failure contract in `tests/determinism/tests/linux_boot_trace.rs` so an immediate `Shutdown`, `InternalError`, or `FailEntry` still fails the test instead of only appearing as `terminal_reason` in the trace artifact.
2. Stop zero-filling detchannel and serial `IN` exits in `prepare_exit_for_trace`; either terminate the trace immediately after recording first detchannel reachability or wire the real ABI handlers before any KVM re-entry.

## Suggestions

1. Gate `write_trace` behind `DH_M9_TRACE_BOOT=1`, or otherwise distinguish non-trace smoke artifacts from the documented opt-in `target/m9/linux_boot_trace.json`.
2. Strengthen `trace_json_reports_required_m9_fields` by parsing the generated artifact as JSON and asserting field values/types rather than relying only on substring checks.
