# Tests

Commands run:

```bash
bd prime
bd show determinism-hypervisor-pee
git diff --check origin/main...HEAD -- crates/dh-worker/src/service.rs
cargo test -p dh-worker --lib m6_accept_capture_neutrality_and_layout_precondition -- --nocapture
```

Results:

- `git diff --check origin/main...HEAD -- crates/dh-worker/src/service.rs`: passed.
- `cargo test -p dh-worker --lib m6_accept_capture_neutrality_and_layout_precondition -- --nocapture`: passed; 1 test, finished in 18.13s on this host.

Interpretation: the targeted test passes, but the pass does not clear the blocking finding. The service-level epoch vectors can be empty, and the test does not fail on that condition.
