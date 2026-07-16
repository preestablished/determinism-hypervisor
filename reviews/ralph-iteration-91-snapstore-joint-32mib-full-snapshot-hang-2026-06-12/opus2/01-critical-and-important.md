# Critical and Important Issues

## Critical

None. The fix is correct and removes the deadlock surface at the root rather than
masking it.

## Important

### I1 — dh-worker joint test drops the `guest_ram_bytes` assertion the client test keeps

- **Severity:** Important (verification completeness, not a code bug)
- **File:** `crates/dh-worker/tests/snapstore_large_put.rs:63`

The two regression tests are meant to be siblings, but the dh-worker one asserts
only the entry *count*:

```rust
assert_eq!(manifest.entries.len() as u64, PAGES);
```

whereas the client-side test (`../snapshot-store/.../test_cases.rs`) additionally
asserts the FULL-manifest invariant:

```rust
assert_eq!(manifest.entries.len() as u64, n_pages);
assert_eq!(manifest.guest_ram_bytes, n_pages * 4096);   // <- missing in dh-worker
```

`entries.len() == PAGES` and `guest_ram_bytes == PAGES * 4096` are independent
facts for a FULL snapshot (the manifest builder validates one against the other,
but the test should pin both ends of the roundtrip). Since the dh-worker test is
the one explicitly flagged as "MATTERS FOR 9sb" (the 128 MiB perf acceptance), it
is the test you least want to be the weaker of the pair. This is the manifest
contract the snapstore-manifest crate documents (FULL: entries cover exactly
`0..guest_ram_bytes/4096`, `lib.rs:47`), so asserting it is exercising the
*contract*, not an implementation detail (cf. `rust-integration-testing.md`:
"Do tests exercise the contract, not the implementation's internals?").

**Suggested fix** (`snapstore_large_put.rs`, after the existing assert):

```rust
let manifest = snapstore_manifest::Manifest::decode(&container).expect("manifest");
assert_eq!(manifest.entries.len() as u64, PAGES);
assert_eq!(manifest.guest_ram_bytes, PAGES * PAGE as u64);
```

This is a one-line strengthening; I rate it Important only because this specific
test is the designated guard for the 128 MiB path and parity with its sibling is
cheap insurance. If the team prefers to treat it as a Suggestion, that's
defensible — there is no correctness bug today.
