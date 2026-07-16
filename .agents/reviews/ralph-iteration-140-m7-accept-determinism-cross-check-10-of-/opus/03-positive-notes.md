# Positive Notes

- `crates/dh-worker/tests/m7_fork_verify.rs:210` evenly samples the configured job universe, including both endpoints for the default 1000-job case, and `crates/dh-worker/tests/m7_fork_verify.rs:684` pins that selection with a non-ignored unit test.

- `crates/dh-worker/tests/m7_fork_verify.rs:623` uses identical explicit child seeds in a single two-child `Fork` call, then `crates/dh-worker/tests/m7_fork_verify.rs:640` asserts the twins actually landed on distinct slots before treating the run as a cross-slot check.

- `crates/dh-worker/tests/m7_fork_verify.rs:647` validates each twin's DHILOG lineage against the root snapshot before replay, so the test is not only comparing final hashes from opaque runs.

- `crates/dh-worker/tests/m7_fork_verify.rs:653` sends both twins through `VerifyReplay`, and `crates/dh-worker/tests/m7_fork_verify.rs:663` compares snapshot refs, state hashes, and input log IDs after replay succeeds.

- `crates/dh-worker/tests/m7_fork_verify.rs:792` gives the cross-slot test its stricter three-slot prerequisite while preserving the existing local-smoke skip path through `DH_M7_ACCEPT_ALLOW_SKIP=1`.

- `docs/ops/test-partitioning.md:61` adds the operator command in the same kvm-intel-gated table as the existing M7 acceptance and nightly canary entries.
