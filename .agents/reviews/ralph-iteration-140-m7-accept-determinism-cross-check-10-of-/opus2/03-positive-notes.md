# Positive Notes

- `crates/dh-worker/tests/m7_fork_verify.rs:623` uses identical explicit entropy seeds for the twin fork, which correctly removes PRNG-continuation ambiguity from the comparison.
- `crates/dh-worker/tests/m7_fork_verify.rs:640` checks that the two returned child leases have distinct slot IDs before accepting the comparison as cross-slot.
- `crates/dh-worker/tests/m7_fork_verify.rs:648` runs the lineage validation for each twin, so both child logs must independently splice from the same root snapshot to their own child snapshot.
- `crates/dh-worker/tests/m7_fork_verify.rs:653` sends both twins through `VerifyReplay`, not just the first child, and `verify_child` rejects divergence and missing terminal `Done` messages.
- `crates/dh-worker/tests/m7_fork_verify.rs:663` compares snapshot hash, state hash, and input log ID, covering the most important externally persisted identities for this acceptance layer.
- `crates/dh-worker/tests/m7_fork_verify.rs:684` adds a cheap non-ignored unit test for the sampled index selection, including small-job edge cases.
- `docs/ops/test-partitioning.md:61` puts the new operator-run command in the same hardware-gated table as the rest of the M7 acceptance workflow.
