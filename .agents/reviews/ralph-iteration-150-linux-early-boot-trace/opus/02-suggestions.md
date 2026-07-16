# Suggestions

### Suggestion: Avoid writing a trace artifact during non-trace smoke runs

File: `tests/determinism/tests/linux_boot_trace.rs:90`

Why: The ignored smoke test now writes `target/m9/linux_boot_trace.json` even when `DH_M9_TRACE_BOOT` is not set. That stale artifact can be mistaken for the opt-in characterization output from the documented acceptance command.

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

### Suggestion: Parse the generated artifact in the host-only serializer test

File: `tests/determinism/tests/linux_boot_trace.rs:645`

Why: The current test checks substrings, which protects field presence but not JSON validity or field types. Parsing the generated string would catch malformed comma/escaping changes while keeping the test host-runnable.

Suggested snippet:

```rust
let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid trace json");
assert_eq!(parsed["schema_version"], 1);
assert_eq!(parsed["total_exits"], 2);
assert_eq!(parsed["lapic_required"], true);
assert_eq!(parsed["denied_msr_indices"][0], "0x1b");
```
