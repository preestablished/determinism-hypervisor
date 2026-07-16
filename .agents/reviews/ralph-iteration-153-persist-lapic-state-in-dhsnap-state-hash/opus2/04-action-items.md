# Action Items

## Critical

- [ ] None.

## Important

- [ ] `crates/dh-worker/src/replay_engine.rs:377`: Change replay bisection probe capture to `capture_bisection_checkpoint_snapshot_with_lapic(..., &rail.lapic, ...)` so VerifyReplay evidence snapshots preserve live LAPC state.

## Suggestions

- [ ] `crates/dh-worker/tests/replay_engine.rs:688`: Add a bisection-enabled LAPC regression that decodes the bisection `reg_diff` payload and asserts a `lapic` diff is present.
- [ ] `crates/dh-vmm/src/lapic.rs:522`: Add an invariant assertion that `LocalApic::from_lapc_section(LapcSection::default()) == LocalApic::new()`.
- [ ] `crates/dh-worker/tests/m4_transparency.rs:18`: Narrow the stale hash-scope comment so it says this test rig passes no device-hash callback while production record/replay folds LAPC into hashes.
