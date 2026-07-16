# Positive Notes

- `crates/dh-vmm/src/runctl.rs:273` preserves the existing `run_segment_with_scheduled_inputs` API by delegating to the new frame-aware variant with an empty frame list.

- `crates/dh-vmm/src/runctl.rs:478` applies frame-scheduled inputs only after `on_exit` services the frame-counter MMIO write, so the AUX `FRAME_MARK` lands before the canonical input that used that frame boundary.

- `crates/dh-worker/src/service.rs:566` validates `at_frame` against the runtime's current absolute frame counter, which matches the public proto model that callers schedule from `RestoreSnapshotResponse` / `TakeSnapshotResponse`.

- `crates/dh-worker/src/service.rs:630` bounds `dev_event.payload` before it reaches the DHILOG writer, avoiding a late `PayloadTooLong` from the logging layer.

- `crates/dh-worker/src/service.rs:1803` refreshes `runtime.position.frame_counter` from the live pv-pad device after each run rather than inferring it from `frames_elapsed`, which is more robust across restore and unusual run modes.

- `crates/dh-vmm/src/recording.rs:361` adds a focused test that proves `DeviceRail::apply_dev_event` emits a canonical `DEV_EVENT` record with the requested fields.
