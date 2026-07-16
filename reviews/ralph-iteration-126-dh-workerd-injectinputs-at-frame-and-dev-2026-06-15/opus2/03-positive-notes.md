# Positive Notes

- `crates/dh-worker/src/runtime.rs:370` replaces the old single `icount` field with `QueuedInputAt`, which makes the frame-vs-icount distinction explicit and avoids sentinel encodings in runtime state.

- `crates/dh-worker/src/service.rs:619` validates `dev_event.device_id`, `event_type`, and payload size before enqueueing, which is the right place to reject malformed wire values.

- `crates/dh-vmm/src/recording.rs:211` keeps DEV_EVENT writing inside `DeviceRail`, preserving the existing pattern where canonical input application and DHILOG recording are paired behind one run-control-facing method.

- `crates/dh-worker/src/service.rs:1803` now samples the actual pv-pad device state after a successful run instead of deriving frame counter progress from `frames_elapsed`, which is better for restore/fork continuity and avoids saturating arithmetic hiding drift.

- `crates/dh-vmm/src/runctl.rs:273` preserves the existing `run_segment_with_scheduled_inputs` API by delegating to the new frame-aware function with an empty frame list, keeping existing callers isolated from the new feature.

- `crates/dh-vmm/src/runctl.rs:1711` adds a live test for frame-triggered input landing at the matching frame mark, which is valuable because this behavior depends on KVM exit timing rather than pure data mapping.
