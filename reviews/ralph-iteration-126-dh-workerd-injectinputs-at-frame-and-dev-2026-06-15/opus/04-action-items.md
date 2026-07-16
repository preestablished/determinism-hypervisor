## Action Items

### Critical
- [ ] [crates/dh-worker/src/service.rs:619] Do not accept `InjectInputs.dev_event` until replay/device semantics are wired, or implement replay support and add end-to-end record/replay coverage.

### Important
- [ ] [crates/dh-worker/src/service.rs:566] Reject or otherwise reserve `at_frame = u32::MAX` so `PadSet.frame_hint` cannot collide with `FRAME_HINT_NONE`.
- [ ] [crates/dh-vmm/src/runctl.rs:494] Wire deterministic IRQ delivery for frame-scheduled inputs, or reject frame-scheduled inputs that may queue vectors before they mutate/log state.
- [ ] [crates/dh-vmm/src/runctl.rs:465] Enforce strictly increasing absolute `FRAME_COUNTER` values before applying `at_frame` inputs.

### Suggestions
- [ ] [crates/dh-worker/src/service.rs:2233] Add service-level tests for mixed icount/frame queued input ordering and retention.
- [ ] [crates/dh-worker/src/service.rs:1808] Use a set for consumed input order removal if queued input volume can grow.
- [ ] [crates/dh-vmm/src/runctl.rs:145] Clarify that `ScheduledFrameInput.index` indexes the caller's input payload array.
- [ ] [crates/dh-worker/src/service.rs:620] Add mapper tests for `dev_event.device_id` and `dev_event.event_type` overflow validation.
