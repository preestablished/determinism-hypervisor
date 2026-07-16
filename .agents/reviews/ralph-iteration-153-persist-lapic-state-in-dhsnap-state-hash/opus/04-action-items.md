# Action Items

## Critical

- None.

## Important

- Update `crates/dh-worker/src/replay_engine.rs:377` to call `capture_bisection_checkpoint_snapshot_with_lapic(...)` and pass `&rail.lapic` so VerifyReplay actual probe snapshots preserve live LAPIC state.
- Add a regression test for the replay bisection probe path that mutates LAPIC state before probe capture and verifies the actual probe DHSNAP contains that non-reset `LAPC` state.

## Suggestions

- Extend `crates/dh-snapshot/tests/dhsnap_codec.rs:353` with a `LapcSection::decode` negative case for nonzero reserved bytes.
- Add a VerifyReplay bisection diagnostics assertion for LAPIC-only divergence, checking that the bisection `reg_diff` names `lapic` and carries distinct encoded LAPC bytes.
