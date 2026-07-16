# Action Items

## Critical

- None.

## Important

- Strengthen `crates/dh-worker/tests/m7_fork_verify.rs:624` so the acceptance run exercises more than the same first two reusable child slots, or rename/document the test as same-fork twin equivalence rather than broad rerun-on-different-slot determinism.
- Add post-fork cleanup in `crates/dh-worker/tests/m7_fork_verify.rs:640` so any assertion or validation failure after a successful `Fork` destroys child leases and then attempts root cleanup before panicking.

## Suggestions

- Reject `DH_M7_CROSS_CHECKS=0` explicitly in `crates/dh-worker/tests/m7_fork_verify.rs:198` instead of silently clamping it to one check.
- Compare fetched input log payload bytes directly in `crates/dh-worker/tests/m7_fork_verify.rs:648` in addition to comparing `input_log_id`.
- Expand `docs/ops/test-partitioning.md:61` to mention `DH_M7_CROSS_CHECKS`, the default sample count, and what `DH_M7_ACCEPT_JOBS` universe is being sampled.
