# Suggestions

### S1. Add a bisection regression for non-reset LAPC probe capture

File: `crates/dh-worker/tests/replay_engine.rs:688`

Rationale: The new non-bisection VerifyReplay tests prove that LAPC participates in replay hash equality, but they do not exercise the bisection evidence path that captures the actual probe snapshot. A regression should fail if the replay probe falls back to reset LAPC.

Suggested snippet:

```rust
let diffs: Vec<dh_worker::snapshot_compare::RegDiff> =
    postcard::from_bytes(&divergence.reg_diff).unwrap();
assert!(
    diffs.iter().any(|diff| diff.name == "lapic"),
    "bisection evidence should carry the LAPC mismatch"
);
```

### S2. Pin the reset-state invariant between LAPC encoding and `LocalApic`

File: `crates/dh-vmm/src/lapic.rs:522`

Rationale: `LapcSection::default()` and `LocalApic::new()` now jointly define legacy empty-LAPC compatibility. A focused assertion would make future default drift obvious without having to infer it through snapshot fixtures.

Suggested snippet:

```rust
assert_eq!(
    LocalApic::from_lapc_section(LapcSection::default()).unwrap(),
    LocalApic::new()
);
```

### S3. Narrow the stale M4 hash-scope comment

File: `crates/dh-worker/tests/m4_transparency.rs:18`

Rationale: The comment still describes device and bus state as outside the hash chain in a broad way. That remains true for this old M4 test rig because it passes no device hash callback, but production record/replay now folds deterministic LAPC state into the hash.

Suggested snippet:

```rust
//! This M4 rig still passes no device hash callback and never touches MMIO;
//! production record/replay paths now fold deterministic LAPC into hashes.
```
