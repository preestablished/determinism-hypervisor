# Action Items

## Critical

- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:172`] Make the ignored acceptance invocation fail loudly when KVM, CPU topology, or the configured 64 slot cores are unavailable instead of returning `None` and letting the test pass.

## Important

- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:318`] Ensure `run_snapshot_destroy` calls `DestroyVm` for the slot on every failure path after a lease has been acquired.

- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:432`] Rework the per-slot fanout helpers to await every spawned task, retain every acquired lease, and destroy all leases that were not already destroyed before returning an error.

- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:238`] Add cleanup guarding to `create_base_snapshot` so the base lease is destroyed if base `TakeSnapshot` fails.

## Suggestions

- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:354`] Assert that `fb_lz4` is empty and `fb_info` is absent when `CaptureSpec.framebuffer` is false.

- [ ] [`crates/dh-worker/tests/m6_full_api_uds.rs:546`] Add explicit post-restore assertions for `slots_free == 0` and unique lease slot ids.

- [ ] [`crates/dh-worker/Cargo.toml:41`] Move `hyper-util` and `tower` to workspace dependency pins or document why these test-only version literals stay local.
