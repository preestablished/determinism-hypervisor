## Action Items

### Critical
- [ ] [crates/dh-vmm/src/runctl.rs:494] Wire deterministic IRQ vector delivery for frame-scheduled inputs, or reject vector-capable frame inputs before mutating/logging device state.

### Important
- [ ] [crates/dh-worker/src/service.rs:566] Reject `at_frame` events when the machine config has no pv-pad device to produce `FRAME_COUNTER` exits.
- [ ] [crates/dh-vmm/src/runctl.rs:465] Enforce the pv-pad `FRAME_COUNTER` monotonic/contiguous contract before counting frames or applying frame-scheduled inputs.
- [ ] [crates/dh-worker/src/service.rs:619] Do not accept public `dev_event` inputs until replay/device mutation semantics are wired, or implement those semantics in the same change.

### Suggestions
- [ ] [crates/dh-worker/src/service.rs:2234] Add RPC-level integration coverage for `InjectInputs(at_frame)` through `Run` and queue consumption.
- [ ] [crates/dh-worker/src/service.rs:1805] Use a `HashSet` for consumed input-order removal.
- [ ] [crates/dh-vmm/src/runctl.rs:458] Factor frame scheduling state out of the exit closure before adding more frame behavior.
