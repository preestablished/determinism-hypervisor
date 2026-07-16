# Tests Run

Commands run:

```bash
bd prime
bd show determinism-hypervisor-pee
git diff --check origin/main...HEAD
cargo test -p dh-worker m6_accept_capture_neutrality_and_layout_precondition -- --nocapture
```

Results:

- `git diff --check origin/main...HEAD`: passed.
- `cargo test -p dh-worker m6_accept_capture_neutrality_and_layout_precondition -- --nocapture`: passed.

Important interpretation: the passing targeted test does not clear the epoch-hash acceptance risk. The service run path currently uses a no-op epoch sink, and the test does not assert that service DHILOG epoch vectors are non-empty.

