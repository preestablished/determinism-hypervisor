# Suggestions

File: `crates/dh-worker/tests/m6_full_api_uds.rs:555`

Reason: The post-restore assertion proves there are 64 paused slots, but it does not verify that the listed slots correspond to the leases just restored or that every slot is based on the expected base snapshot. The fresh worker makes unrelated paused slots unlikely, but stronger assertions would make failures easier to diagnose.

Suggested fix: Compare the listed `slot_id`s to the restored leases and assert each `base_snapshot_id` matches the baseline snapshot hash.

File: `crates/dh-worker/tests/m6_full_api_uds.rs:432`

Reason: The test spawns 64 tasks, but there is no barrier that makes all clients enter each phase at the same time. It does hold 64 slots concurrently after restore, so this is not blocking, but a barrier would better stress the UDS/gRPC and slot-manager concurrency intended by the acceptance description.

Suggested fix: Add a `tokio::sync::Barrier` per phase and wait inside each spawned task immediately before the RPC under test.

File: `crates/dh-worker/Cargo.toml:41`

Reason: `hyper-util` and `tower` are added as literal dev-dependency versions while most of this workspace centralizes versions. Direct versions make future tonic/hyper upgrades easier to drift.

Suggested fix: Move these to workspace dependencies, or reuse an existing test connector helper if one already owns the UDS tonic dependency shape.
