# Critical And Important Issues

## Critical: `dev_event` inputs create unreplayable logs

- Severity: Critical
- File: `crates/dh-worker/src/service.rs:619`
- Supporting references: `crates/dh-vmm/src/recording.rs:213`, `crates/dh-worker/src/replay_engine.rs:287`

`InjectInputs` now accepts `ScheduledEvent.dev_event`, queues it, and records it through `DeviceRail::apply_dev_event`. However the replay engine still rejects every canonical `DEV_EVENT` with `ReplayError::NotYetWired("DEV_EVENT replay needs the detchannel composition (ol1)")`. Before this branch, the worker returned `UNIMPLEMENTED` for this API tail; after this branch, a successful public request can produce a DHILOG segment that cannot be replayed or verified by the repo's replay path.

This breaks the core "record now, replay later" contract for accepted canonical inputs. `DeviceRail::apply_dev_event` also only writes the log record; it does not apply any device-side mutation, so enabling generic device events needs a symmetric live/replay device semantics story, not just a codec call.

Suggested fix, safest until replay is implemented:

```rust
WireEvent::DevEvent(_) => {
    return Err(unimplemented_status("InjectInputs dev_event"));
}
```

If this feature must land now, implement replay support in `replay_engine.rs` and add an end-to-end record/replay test that creates a segment containing the accepted `DEV_EVENT` and proves replay succeeds. Research reference: `/home/infra-admin/.claude/research/rust-integration-testing.md` recommends exercising the API contract and failure paths, not only mapper happy paths.

## Important: `at_frame = u32::MAX` collides with the `FRAME_HINT_NONE` sentinel

- Severity: Important
- File: `crates/dh-worker/src/service.rs:566`
- Supporting reference: `crates/dh-inputlog/src/dhilog.rs:65`

For `PadSet`, frame-scheduled inputs are logged with `frame_hint = frame`. The DHILOG sentinel for "not frame-scheduled" is `FRAME_HINT_NONE == 0xFFFF_FFFF`, but the mapper currently accepts `at_frame = u32::MAX`. That makes a real frame-scheduled `PadSet` indistinguishable from a non-frame-scheduled `PadSet` in the canonical log metadata.

Suggested fix:

```rust
WireAt::AtFrame(frame) => {
    if *frame == dh_inputlog::dhilog::FRAME_HINT_NONE {
        return Err(Status::invalid_argument(format!(
            "events[{index}].at_frame value {frame} is reserved"
        )));
    }
    if *frame <= current_frame_counter {
        return Err(Status::invalid_argument(format!(
            "events[{index}].at_frame must be greater than current frame_counter {current_frame_counter}, got {frame}"
        )));
    }
    (QueuedInputAt::Frame(*frame), *frame)
}
```

Research reference: `/home/infra-admin/.claude/research/rust-newtype-validation.md` calls out that domain values with invariants or reserved sentinels should be validated at the boundary, ideally with a typed wrapper.

## Important: frame-scheduled inputs that queue IRQ vectors fault after mutating/logging

- Severity: Important
- File: `crates/dh-vmm/src/runctl.rs:494`
- Supporting reference: `crates/dh-worker/src/service.rs:695`

The frame-input path calls `input_sink`, which applies the input to device state and records it. If the input queues an IRQ vector, run control returns an error because frame IRQ delivery is not wired. `PadSet` and `NetRx` can both legally return vectors depending on guest device configuration, so a valid accepted request can mutate/log the rail and then fault the slot.

That is fail-loud, but it is still a bad API edge: the request is accepted up front and only fails at the frame boundary after side effects have happened.

Suggested fix: either wire deterministic vector delivery for frame-triggered inputs at that same boundary, or reject frame-scheduled input kinds that may queue vectors until the delivery path exists. A fail-fast interim guard is preferable to applying the input and then faulting:

```rust
if matches!(at, QueuedInputAt::Frame(_))
    && matches!(wire_event, WireEvent::PadSet(_) | WireEvent::NetRx(_))
{
    return Err(unimplemented_status(
        "InjectInputs at_frame inputs that may queue IRQ vectors",
    ));
}
```

Add a test with pad IRQ or net IRQ enabled so this edge is covered explicitly. Research reference: `/home/infra-admin/.claude/research/rust-integration-testing.md` highlights failure-path coverage for documented behavior.

## Important: `at_frame` trusts raw frame writes without enforcing the strict monotonic frame invariant

- Severity: Important
- File: `crates/dh-vmm/src/runctl.rs:465`
- Supporting reference: `crates/dh-devices/src/pad.rs:15`

The new frame input path applies inputs on any `FRAME_COUNTER` MMIO write whose payload equals the scheduled frame. The pv-pad contract says the counter is lineage-absolute and strictly increasing, and `pad.rs` explicitly says monotonicity checks belong in run control, but this path does not enforce that invariant. A guest that writes frame `12` and later frame `11` can still trigger an `at_frame = 11` input after the lineage frame counter already advanced, which violates the scheduling model the worker validates against at queue time.

Suggested fix: pass the starting absolute frame counter into run control and reject non-increasing frame marks before applying frame inputs.

```rust
let mut last_frame = start_frame_counter;

if let Some(frame) = frame_mark {
    if frame <= last_frame {
        return Err(BoundaryError::Exit(format!(
            "FRAME_COUNTER must increase monotonically: previous {last_frame}, got {frame}"
        )));
    }
    last_frame = frame;
    // Existing frame input application follows.
}
```

Research reference: `/home/infra-admin/.claude/research/rust-newtype-validation.md` is relevant because `ScheduledFrameInput.frame` carries an invariant that is currently represented as a raw `u32`.
