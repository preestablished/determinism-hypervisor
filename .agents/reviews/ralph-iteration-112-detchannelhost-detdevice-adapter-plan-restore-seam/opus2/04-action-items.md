# Action Items

## Critical

- None

## Important

- [ ] `crates/dh-devices/src/detchannel.rs:318` Make EVTC restore reject non-canonical flags, nonzero payload bytes under absent options, invalid `init_status` values, and impossible attached/detached status combinations.
- [ ] `crates/dh-devices/src/detchannel.rs:1617` Add negative EVTC restore tests for corrupted option flags, stale option payload bytes, and invalid status values.

## Suggestions

- [ ] `crates/dh-worker/tests/fork_engine.rs:107` Add a detchannel-equipped fork test proving the child bus restores EVTC against the child slot memory handle.
- [ ] `crates/dh-worker/tests/restore_engine.rs:37` Centralize or document the detchannel adapter's intended MMIO base and production composition status.
- [ ] `crates/dh-devices/src/detchannel.rs:199` Delay restore-plan factory invocation until after basic EVTC version/length validation, or document that factories must be pure on failed restores.
- [ ] `crates/dh-worker/tests/restore_engine.rs:694` Add a malformed-EVTC DHSNAP mutation to the generic restore-engine rejection suite.
