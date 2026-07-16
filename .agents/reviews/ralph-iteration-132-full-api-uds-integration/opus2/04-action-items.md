# Action Items

## Critical

- [ ] None.

## Important

- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:172`] Make the explicit M6 acceptance invocation fail, not pass, when KVM or the required 64 slot cores are unavailable.
- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:318`] Add best-effort `DestroyVm` cleanup for every acquired lease on all error paths, including partial failures in restore, inject, run, and snapshot phases.

## Suggestions

- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:555`] Validate restored `slot_id`s and `base_snapshot_id`s in the `ListSlots` check.
- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:432`] Use a per-phase barrier if the acceptance test should stress simultaneous UDS/gRPC entry, not just concurrent slot occupancy.
- [ ] [`crates/dh-worker/Cargo.toml:41`] Prefer workspace-pinned dev dependencies for `hyper-util` and `tower`.
