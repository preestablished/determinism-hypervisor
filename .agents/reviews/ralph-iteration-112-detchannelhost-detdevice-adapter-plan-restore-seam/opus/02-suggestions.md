# Suggestions

## `crates/dh-devices/src/detchannel.rs:199`

`DetChannelDevice::restore` delegates directly to `DetChannelHost::restore`, which currently accepts non-canonical EVTC flag bytes because `bytes[12] == 1`, `bytes[17] == 1`, and `bytes[22] == 1` treat every other value as false at `crates/dh-devices/src/detchannel.rs:318`, `crates/dh-devices/src/detchannel.rs:319`, and `crates/dh-devices/src/detchannel.rs:321`. Now that EVTC is on the generic device restore path, consider making those flag fields strict `0 | 1` values and adding a malformed-EVTC restore test.

This is not a blocker for snapshots produced by this engine, but strict rejection would better match the rest of the restore engine's "malformed snapshot is loud" posture.

## `crates/dh-worker/tests/restore_engine.rs:245`

The KVM acceptance test proves the happy path reattaches after RAM load. It would be useful to add one negative restore-engine case where the EVTC section says "attached" but the restored RAM page does not contain a valid channel header, and assert the restore fails with a device-section codec error. `DetChannelHost` already has a device-level bad-header test at `crates/dh-devices/src/detchannel.rs:1643`; carrying that through `restore_snapshot` would pin the cross-crate contract that EVTC validation is actually happening inside the device loop.

## `crates/dh-worker/src/replay_engine.rs:287`

The adapter resolves the EVTC snapshot/restore seam, but replay still returns `NotYetWired` for `DEV_EVENT` records. If this iteration is considered the last "ol1" detchannel-composition step, file or link the remaining runtime/replay wiring explicitly. I am treating it as follow-up because bead `determinism-hypervisor-abe` is scoped to the adapter and plan-supplying restore seam, not full detchannel event replay.
