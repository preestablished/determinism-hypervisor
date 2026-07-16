# Suggestions

- `crates/dh-worker/tests/m6_full_api_uds.rs:354`: The test validates `feature_bytes`, but `capture_spec` sets `framebuffer: false` and the test does not assert that no framebuffer payload was returned. Add `fb_lz4.is_empty()` and `fb_info.is_none()` checks so a service that ignores the framebuffer flag cannot still pass by matching the baseline.

- `crates/dh-worker/tests/m6_full_api_uds.rs:546`: The post-restore `ListSlots` check strongly implies 64 occupied slots, but clearer diagnostics would help this acceptance gate. Also assert `GetWorkerInfo.slots_free == 0` after restore and that the 64 returned leases have unique slot ids.

- `crates/dh-worker/Cargo.toml:41`: `hyper-util` and `tower` are introduced as direct version literals while this workspace centralizes most shared versions in `[workspace.dependencies]`. Consider adding these to the root workspace dependency table and using `.workspace = true`, or add a short comment explaining why these test-only pins intentionally stay local.
