# Positive Notes

- `crates/dh-worker/src/runtime.rs:80` - `insert_many` validates every target slot before publishing any runtime, so the batch insert has a clear all-or-nothing shape.

- `crates/dh-worker/src/runtime.rs:320` - the new runtime-table tests explicitly cover both occupied-slot rejection and duplicate-slot rejection without leaving partial entries behind.

- `crates/dh-worker/src/service.rs:420` - lifecycle work is placed behind `spawn_blocking`, which is the right boundary for KVM and snapshot-store construction paths that must not occupy async executor workers.

- `crates/dh-worker/src/service.rs:560` - `destroy_runtime_slot` checks `SlotManager::check_destroy` before taking ownership out of the runtime table, then reinserts the runtime if the manager release fails. That preserves KVM ownership on the normal failure path.

- `crates/dh-worker/src/service.rs:526` - fork captures child positions before moving runtimes into `insert_many`, avoiding awkward post-move metadata reconstruction.

- `crates/dh-worker/src/service.rs:1002` - the service tests exercise real runtime ownership through allocate, position publication, destroy, fork rollback, and parent thawing, which is valuable coverage for this layer.
