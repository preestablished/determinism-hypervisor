# Action Items

## Critical

- [ ] None.

## Important

- [ ] None.

## Suggestions

- [ ] Harden `crates/dh-worker/tests/m7_fork_verify.rs:624` so malformed successful `Fork` responses best-effort destroy any returned child leases before returning an error.

- [ ] Consider comparing fetched input log payload bytes in `crates/dh-worker/tests/m7_fork_verify.rs:647` in addition to comparing `input_log_id`.

- [ ] Document `DH_M7_ACCEPT_JOBS` and `DH_M7_CROSS_CHECKS` near the cross-slot command in `crates/dh-worker/tests/m7_fork_verify.rs:22`.

- [ ] Clarify in `docs/ops/test-partitioning.md:61` that the current operator command compares the two child slots selected by the fork allocator, not necessarily every configured child slot.
