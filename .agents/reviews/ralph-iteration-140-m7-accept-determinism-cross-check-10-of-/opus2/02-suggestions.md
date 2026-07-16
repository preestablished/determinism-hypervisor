# Suggestions

## Suggestion 1

File/line: `crates/dh-worker/tests/m7_fork_verify.rs:198`

Rationale: `DH_M7_CROSS_CHECKS=0` currently parses successfully and is silently clamped to one check, despite the panic text saying the value must be positive. For an operator-run acceptance gate, silent broadening/narrowing of the configured sample can make run logs misleading.

Suggested snippet:

```rust
let parsed = value.parse::<usize>().unwrap_or_else(|_| {
    panic!("{CROSS_CHECKS_ENV} must be a positive integer, got {value:?}")
});
assert!(parsed > 0, "{CROSS_CHECKS_ENV} must be a positive integer, got {value:?}");
parsed
```

## Suggestion 2

File/line: `crates/dh-worker/tests/m7_fork_verify.rs:648`

Rationale: The test fetches each input log to validate lineage, but then only compares `input_log_id`. If log IDs are content-derived this is effectively enough; still, direct payload equality would make the acceptance condition independent of store ID semantics and would produce better divergence diagnostics if canonical and non-canonical records ever drift separately.

Suggested snippet:

```rust
let mut checked = Vec::new();
for child in &children {
    let log = tokio::task::block_in_place(|| fetch_log_payload(store, &child.input_log_id));
    validate_single_edge_lineage(root_snapshot, child, &log);
    checked.push((child.slot_id, log));
}

let (_, first_log) = checked.first().expect("at least one checked child");
for (slot_id, log) in checked.iter().skip(1) {
    if log != first_log {
        return Err(format!("cross-slot child {index} input log payload diverged on slot {slot_id}"));
    }
}
```

## Suggestion 3

File/line: `docs/ops/test-partitioning.md:61`

Rationale: The docs add the operator command but do not mention `DH_M7_CROSS_CHECKS`, the default sample size, or the current need for enough slots to run meaningful cross-slot comparison. Operators reading the partitioning table cannot tell whether they are running 10 checks, 100 checks, or the full 1000-job universe.

Suggested snippet:

```markdown
| M7 cross-slot rerun determinism | `DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_CROSS_CHECKS=10 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture` | operator-run; samples 10 evenly spaced indices from `DH_M7_ACCEPT_JOBS` |
```
