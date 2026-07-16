# Critical And Important Issues

## Critical

None found.

## Important

### I1. VerifyReplay bisection probes drop live LAPC state

Severity: Important
File: `crates/dh-worker/src/replay_engine.rs:377`

Problem: `capture_bisection_probe` calls `capture_bisection_checkpoint_snapshot(...)`, whose compatibility wrapper writes a reset `LocalApic::new()` into the DHSNAP. The rest of replay now restores LAPC into the rail and hashes `lapic_section(&rail.borrow().lapic)`, and the service-side recording checkpoint path correctly uses the `_with_lapic` variant. With bisection enabled, an actual probe snapshot taken after LAPC changes will not describe the same LAPC state that produced the hash being diagnosed. That can make `snapshot_compare` report misleading LAPC evidence, or hide the recorded-vs-replay LAPC relationship behind an artificial reset-state probe.

Suggested fix:

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
