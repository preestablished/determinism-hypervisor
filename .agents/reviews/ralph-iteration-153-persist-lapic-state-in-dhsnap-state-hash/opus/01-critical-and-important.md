# Critical And Important Issues

## Critical

None.

## Important

### Replay bisection probe snapshots drop live LAPIC state

- Severity: Important
- Location: `crates/dh-worker/src/replay_engine.rs:377`
- Related location: `crates/dh-worker/src/snapshot_engine.rs:224`
- Problem: `capture_bisection_probe` captures the actual replay probe with `capture_bisection_checkpoint_snapshot(...)`. That wrapper delegates to `_with_lapic` using `LocalApic::new()`, so the actual probe's `LAPC` section is reset even though replay's live device rail has `rail.lapic`. The normal replay hash path now includes LAPIC, but VerifyReplay bisection evidence can compare an expected recorded checkpoint against an actual probe snapshot with the wrong LAPIC bytes. This makes `snapshot_compare` evidence incomplete or misleading for any divergence after LAPIC state changes.
- Suggested fix snippet:

```rust
let snapshot = crate::snapshot_engine::capture_bisection_checkpoint_snapshot_with_lapic(
    slot,
    dh_vmm::SlotState::Paused,
    &rail.bus,
    &rail.lapic,
    &rail.entropy,
    machine_config,
    boundary,
    store,
)
.map_err(ReplayError::BisectionCapture)?;
```

Add a regression test that drives a non-reset replay `rail.lapic`, captures a VerifyReplay bisection probe, and asserts the probe DHSNAP `LAPC` decodes to that non-reset value.
