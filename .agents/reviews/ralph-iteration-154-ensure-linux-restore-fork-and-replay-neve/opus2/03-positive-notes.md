# Positive Notes

- `crates/dh-worker/src/service.rs:1495` and `crates/dh-worker/src/service.rs:3388`: `VerifyReplay` and `RestoreSnapshot` now create a fresh slot, validate the config hash, build the runtime bus, and rely on snapshot restoration rather than pre-booting the machine. That matches the intended snapshot-as-authority model.

- `crates/dh-worker/src/service.rs:2030`: `boot_slot_with_loaders` remains injectable, so loader routing stays unit-testable and the new observer can distinguish ELF from BzImage without duplicating boot logic.

- `crates/dh-worker/tests/replay_engine.rs:889`: The replay acceptance test destroys the original live VM before calling `VerifyReplay`, which avoids a false positive from accidentally reusing live runtime state.

- `crates/dh-worker/tests/restore_engine.rs:909`: The restore acceptance test checks the returned config hash, state hash, restored snapshot, and fork-child snapshot, which exercises the external API result rather than only internal state.

- `crates/dh-worker/tests/restore_engine.rs:1033` and `crates/dh-worker/tests/restore_engine.rs:1048`: Comparing EVTC and BLKO sections on restored and forked snapshots is a useful guard that device sections survive the restore/fork path intact.

- `tools/dh-cli/src/gate.rs:65` and `tools/dh-cli/src/run.rs:73`: Setting `hash_device_sections: None` is the correct Segment construction for these CLI paths because they run nanokernel/serial flows without a runtime bus section hashing context.
