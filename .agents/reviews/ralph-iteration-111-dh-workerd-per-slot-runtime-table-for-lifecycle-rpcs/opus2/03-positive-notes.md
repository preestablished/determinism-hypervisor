# Positive Notes

- `crates/dh-worker/src/runtime.rs:80`: `RuntimeTable::insert_many` validates every target slot before moving any runtime into the table. That preserves all-or-nothing behavior inside the runtime table itself.

- `crates/dh-worker/src/runtime.rs:321`: The new `insert_many` tests cover both collision rollback and duplicate slot ids. These are the two invariants most likely to regress later.

- `crates/dh-worker/src/service.rs:560`: `destroy_runtime_slot` checks manager destroy eligibility before removing the runtime and reinserts the runtime if the final manager destroy fails. That is the right ordering for keeping manager and runtime ownership aligned.

- `crates/dh-worker/src/service.rs:526`: Fork child positions are captured from the built `SlotRuntime`s before moving them into the table, then mirrored into `SlotManager`. This keeps the introspection row driven by the actual runtime object rather than duplicated builder inputs.

- `crates/dh-worker/src/service.rs:1000`: The positive KVM-backed allocated-runtime test checks both runtime occupancy and `SlotManager` position/base-snapshot state, then verifies `DestroyVm` releases both tables. That is a useful end-to-end lifecycle assertion to preserve.

- `crates/dh-worker/src/service.rs:1061`: The fork rollback test explicitly verifies parent thawing and child cleanup after builder failure. That is the right behavior to pin before rfv wires real fork RPCs into these helpers.
