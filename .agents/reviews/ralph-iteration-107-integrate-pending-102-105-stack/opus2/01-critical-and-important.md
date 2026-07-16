Critical: none.

Important:

- `crates/dh-vmm/src/run.rs`: `pin_current_thread` called `CPU_SET` before checking whether the core id fits the fixed `cpu_set_t`, so bad input could panic instead of returning `PinError`.
- `crates/dh-worker/src/slot_manager.rs`: duplicate or overlapping core ranges were accepted, violating the dedicated-core invariant.
- `crates/dh-worker/src/service.rs`: `prepare_uds_path` unconditionally removed the configured path before binding, so a bad `--uds` path could delete a regular file.
