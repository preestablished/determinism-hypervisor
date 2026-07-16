# Suggestions

### Suggestion: Avoid writing the trace artifact during the one-exit smoke path

File: `tests/determinism/tests/linux_boot_trace.rs:90`

Why: When `DH_M9_TRACE_BOOT` is not set, the test now writes `target/m9/linux_boot_trace.json` even though it only collected one exit. That can leave a stale-looking artifact in the same path used by intentional full traces. Keeping artifact writes to explicit trace mode makes the file's presence easier to interpret.

Suggested snippet:

```rust
let trace_path = trace_output_path();
if trace_required() {
    write_trace(&trace, &trace_path).expect("write linux boot trace");
    assert!(
        trace_path.is_file(),
        "{TRACE_BOOT_ENV}=1 must produce {TRACE_OUTPUT}"
    );
}
```

### Suggestion: Use structured JSON construction for the trace schema

File: `tests/determinism/tests/linux_boot_trace.rs:476`

Why: The hand-written JSON is currently careful, but this schema is likely to grow as more early-boot signals are added. A structured writer reduces future comma/escaping mistakes and allows the unit test to parse the artifact instead of checking substrings.

Suggested snippet:

```rust
fn trace_json(trace: &LinuxBootTrace) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": 1,
        "total_exits": trace.total_exits,
        "exit_limit": trace.exit_limit,
        "icount_limit": trace.icount_limit,
        "final_icount": trace.final_icount,
        "terminal_reason": trace.terminal_reason.as_deref().unwrap_or("unknown"),
        "lapic_required": trace.lapic_required(),
        "exit_kind_counts": trace.exit_kind_counts,
    }))
    .expect("serialize linux boot trace")
}
```

### Suggestion: Accept conventional truthy values for trace mode

File: `tests/determinism/tests/linux_boot_trace.rs:423`

Why: `DH_M9_TRACE_BOOT=true` or `yes` currently silently behaves like smoke mode. Since this is an operator-facing ignored test, rejecting unknown values or accepting common truthy spellings would make misconfiguration clearer.

Suggested snippet:

```rust
fn trace_required() -> bool {
    match std::env::var(TRACE_BOOT_ENV).as_deref() {
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES") => true,
        Ok("0") | Ok("false") | Ok("FALSE") | Ok("no") | Ok("NO") | Err(_) => false,
        Ok(raw) => panic!("{TRACE_BOOT_ENV} must be 1/0, true/false, or yes/no; got {raw:?}"),
    }
}
```
