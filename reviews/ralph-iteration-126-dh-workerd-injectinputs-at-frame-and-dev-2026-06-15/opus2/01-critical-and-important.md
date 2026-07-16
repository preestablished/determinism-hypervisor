# Critical And Important Findings

## Critical

### Frame-scheduled inputs that queue IRQ vectors mutate/log, then fault

Severity: Critical

File: `crates/dh-vmm/src/runctl.rs:494`

Description: The frame path applies the queued input first, then rejects any returned vectors with `frame IRQ delivery is not wired`. `PadSet` and `NetRx` can legitimately return an edge IRQ depending on guest device configuration. In that case the device state and DHILOG record have already been mutated, but the run is converted into a fatal boundary error. This makes a valid public `InjectInputs(at_frame=..., pad_set=...)` request slot-fatal whenever the guest enabled pv-pad IRQs, while the icount-scheduled path already has deterministic vector chaining for the same input kind.

Suggested fix: Either wire immediate vector delivery at the frame boundary before the vCPU re-enters, or reject vector-capable frame inputs before applying/logging them. The stronger fix is to make the frame exit unwind to run control with a pending frame boundary, then reuse the existing injection chaining logic:

```rust
// Sketch: frame exit handling records matching inputs and asks the outer loop
// to deliver vectors before continuing toward the original agenda target.
struct PendingFrameVectors {
    boundary: Boundary,
    vectors: Vec<u8>,
}

// After servicing the FRAME_COUNTER exit:
let boundary = Boundary { icount, rip: 0, rcx: 0 };
let mut vectors = Vec::new();
for scheduled in matching_frame_inputs {
    vectors.extend(input_sink(scheduled.index, boundary)?);
}
if !vectors.is_empty() {
    pending_frame_vectors = Some(PendingFrameVectors { boundary, vectors });
    return Err(BoundaryError::Exit("frame input vectors pending".into()));
}
```

Then the outer run loop should deliver `vectors` with `inject_at_boundary`/`step_one_entry` the same way same-icount scheduled inputs are chained today, and only mark the inputs consumed after that succeeds.

Research reference: `/home/infra-admin/.claude/research/rust-integration-testing.md` calls out failure-path coverage; this path needs a test with pv-pad `IRQ_VECTOR` enabled, not only the happy path where no vector is returned.

## Important

### `at_frame` is accepted for machines with no pv-pad frame source

Severity: Important

File: `crates/dh-worker/src/service.rs:566`

Description: `queued_input_from_proto` accepts `AtFrame` based only on the current frame counter value. If the machine config omits `DEVICE_ID_PV_PAD`, `frame_counter_from_bus` returns `0` and no `FRAME_COUNTER` MMIO exits will ever occur, so the worker acknowledges the input as scheduled even though it cannot land. This is an API validation gap that can leave inputs pending indefinitely.

Suggested fix:

```rust
WireAt::AtFrame(frame) => {
    if !config.device_set.contains(&dh_devices::pad::DEVICE_ID_PV_PAD) {
        return Err(Status::failed_precondition(format!(
            "events[{index}].at_frame requires pv-pad in MachineConfig.device_set"
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

Research reference: `/home/infra-admin/.claude/research/rust-newtype-validation.md` emphasizes enforcing invariants at construction/deserialization boundaries. `QueuedInputAt::Frame` is a domain value with a real device precondition, so the boundary should reject invalid states.

### Frame counter monotonicity is still trusted but now drives input delivery

Severity: Important

File: `crates/dh-vmm/src/runctl.rs:465`

Description: Run control decodes any 4-byte write to the pv-pad `FRAME_COUNTER` GPA and applies matching frame inputs based solely on the written value. The pv-pad docs say the counter is lineage-absolute and strictly increasing, and that monotonicity checks belong in run control. Without enforcing that here, a guest can write a target frame value early, duplicate it, or regress then replay it, causing host inputs to land at a different logical frame than the API contract promises.

Suggested fix: Carry the segment's starting frame counter into run control and reject non-contiguous frame writes before applying frame inputs or counting frame budget progress.

```rust
let mut last_frame = start_frame_counter;

if let Some(frame) = frame_mark {
    let expected = last_frame.checked_add(1).ok_or_else(|| {
        BoundaryError::Exit("FRAME_COUNTER overflow".into())
    })?;
    if frame != expected {
        return Err(BoundaryError::Exit(format!(
            "FRAME_COUNTER contract violation: expected {expected}, got {frame}"
        )));
    }
    last_frame = frame;
    frames_seen += 1;
    // apply matching frame inputs...
}
```

Research reference: `/home/infra-admin/.claude/research/rust-newtype-validation.md`; this is another domain invariant that should be represented and checked where raw guest/proto values cross into scheduling logic.

### `dev_event` can now create logs that replay still rejects

Severity: Important

File: `crates/dh-worker/src/service.rs:619`

Description: `InjectInputs` now accepts generic `dev_event` and `DeviceRail::apply_dev_event` records it, but replay still returns `NotYetWired` for any canonical `DEV_EVENT`. That means a public worker request can create a sealed input log that the project replay engine cannot consume. There is also no device mutation paired with the generic event, so the semantics are currently "record-only" despite the DEV_EVENT contract describing host-side device mutation.

Suggested fix: Either keep `InjectInputs dev_event` unimplemented until replay/device mutation semantics are wired, or add replay support in the same branch. If keeping it closed for now:

```rust
WireEvent::DevEvent(_) => {
    return Err(unimplemented_status(
        "InjectInputs dev_event replay/device mutation semantics"
    ));
}
```

Research reference: `/home/infra-admin/.claude/research/rust-integration-testing.md`; this needs integration coverage that records a dev event and verifies the resulting log through the replay path, not only a mapper/unit test.
