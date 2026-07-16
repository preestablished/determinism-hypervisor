# Suggestions

## Add RPC-level coverage for `InjectInputs(at_frame)`

File: `crates/dh-worker/src/service.rs:2234`

The new tests cover mapper behavior and a runctl-only live frame input, but they do not prove the worker RPC path wires `InjectInputs -> queued_inputs -> Run -> consumed queue` for `at_frame`. Add an integration-style service test using a frame-emitting guest so the public API contract is covered end to end.

Suggested shape:

```rust
let injected = svc.inject_inputs(Request::new(proto::InjectInputsRequest {
    lease: Some(lease.clone()),
    events: vec![proto::ScheduledEvent {
        at: Some(proto::scheduled_event::At::AtFrame(start_frame + 2)),
        event: Some(proto::scheduled_event::Event::PadSet(proto::PadSet {
            port: 0,
            buttons: 0xA5A5,
        })),
    }],
})).await?;

let run = svc.run(Request::new(proto::RunRequest {
    lease: Some(lease),
    until: Some(proto::run_request::Until::FrameBudget(3)),
    hard_icount_cap: 50_000_000,
    capture: None,
})).await?;
```

Research reference: `/home/infra-admin/.claude/research/rust-integration-testing.md`; the risk is at the public integration boundary rather than in the pure mapper alone.

## Factor frame input scheduling state out of the exit closure

File: `crates/dh-vmm/src/runctl.rs:458`

The `exits!` closure now handles halt, frame decoding, exit service, frame input application, frame-budget stop, and sdk-event stop. A small helper/state object for frame scheduling would make future fixes, especially vector delivery and monotonic frame validation, less fragile.

Suggested shape:

```rust
struct FrameInputState {
    applied: Vec<bool>,
    last_frame: u32,
}

impl FrameInputState {
    fn apply_matching(
        &mut self,
        frame: u32,
        icount: u64,
        frame_inputs: &[ScheduledFrameInput],
        input_sink: &mut dyn FnMut(usize, Boundary) -> Result<Vec<u8>, BoundaryError>,
    ) -> Result<Vec<u8>, BoundaryError> {
        // validate frame, apply matching inputs, return vectors
    }
}
```

## Use a set when removing consumed input orders

File: `crates/dh-worker/src/service.rs:1805`

`retain(|input| !consumed_input_orders.contains(&input.order))` is quadratic in the number of queued and consumed inputs. It is not urgent, but `InjectInputs` is a public batching API, and this is a cheap maintainability improvement.

Suggested snippet:

```rust
let consumed: std::collections::HashSet<u64> =
    consumed_input_orders.iter().copied().collect();
runtime
    .queued_inputs
    .retain(|input| !consumed.contains(&input.order));
```

## Consider validated constructors for queued input timing

File: `crates/dh-worker/src/runtime.rs:370`

`QueuedInputAt` is a clearer representation than a single `icount`, but both variants are public raw values. As more timing invariants accumulate (`icount > current`, frame source exists, frame is future), consider a small constructor on the worker side that centralizes validation rather than allowing ad hoc construction.

Research reference: `/home/infra-admin/.claude/research/rust-newtype-validation.md`.
