# Suggestions

### Add a codec-level reserved-byte negative for LAPC

- Location: `crates/dh-snapshot/tests/dhsnap_codec.rs:353`
- Rationale: `LapcSection::decode` correctly rejects nonzero reserved bytes at `crates/dh-snapshot/src/dhsnap.rs:461`, but the shared decode-negative test currently exercises only TIME and ENTR errors. A direct test would pin the reserved-byte contract for the new stable layout.
- Suggested snippet:

```rust
let mut lapc = LapcSection::default().encode();
lapc[10] = 1;
assert_eq!(
    LapcSection::decode(&lapc, LapcSection::VERSION),
    Err(SectionError::NonzeroReserved { offset: 10 })
);
```

### Exercise bisection diagnostics for LAPC divergence

- Location: `crates/dh-worker/tests/replay_engine.rs:687`
- Rationale: The added replay tests prove LAPIC hash divergence is detected, but they stop at the high-level divergence event. A targeted bisection-path assertion would have caught the reset-LAPIC probe capture gap and would keep the diagnostic contract pinned.
- Suggested snippet:

```rust
// After forcing a LAPIC-only divergence, inspect the divergence payload and
// assert the bisection reg_diff includes `lapic` with distinct LAPC bytes.
assert_eq!(reg_diff.as_ref().map(|d| d.name.as_str()), Some("lapic"));
```
