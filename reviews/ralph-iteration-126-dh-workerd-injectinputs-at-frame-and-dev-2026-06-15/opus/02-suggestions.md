# Suggestions

## Add RPC-level coverage for mixed queued inputs

- File: `crates/dh-worker/src/service.rs:2233`

The new mapper tests cover direct `queued_input_from_proto` cases, but not the full RPC queue ordering with a mix of icount and frame inputs. A service-level test that queues both kinds and asserts the stored order and retained pending inputs would make regressions easier to catch.

```rust
assert_eq!(
    runtime.queued_inputs.iter().map(|i| i.at).collect::<Vec<_>>(),
    vec![
        QueuedInputAt::Icount(150),
        QueuedInputAt::Frame(12),
    ],
);
```

## Avoid linear `contains` while removing consumed inputs

- File: `crates/dh-worker/src/service.rs:1808`

`retain(|input| !consumed_input_orders.contains(&input.order))` is fine for small queues, but it is quadratic as the number of queued inputs grows. Converting consumed orders to a set is a small maintainability improvement.

```rust
let consumed: std::collections::BTreeSet<_> =
    consumed_input_orders.iter().copied().collect();
runtime.queued_inputs.retain(|input| !consumed.contains(&input.order));
```

## Clarify what `ScheduledFrameInput.index` indexes

- File: `crates/dh-vmm/src/runctl.rs:145`

`index` is not an index into `frame_inputs`; it is the caller-owned payload index passed back to `input_sink`. A short doc tweak would prevent accidental misuse by future callers.

```rust
/// `index` is passed unchanged to `input_sink`; it indexes the caller's
/// queued-input payload array, not this `frame_inputs` slice.
```

## Add mapper tests for numeric overflow fields

- File: `crates/dh-worker/src/service.rs:620`

The mapper validates `dev_event.device_id` and `dev_event.event_type` with `u16::try_from`, but the added tests only cover the payload length failure path. Add explicit overflow cases so the public boundary validation stays pinned.

```rust
let err = queued_input_from_proto(
    2,
    &proto::ScheduledEvent {
        at: Some(proto::scheduled_event::At::AtIcount(150)),
        event: Some(proto::scheduled_event::Event::DevEvent(proto::DeviceEvent {
            device_id: u32::from(u16::MAX) + 1,
            event_type: 1,
            payload: Vec::new(),
        })),
    },
    100,
    10,
    &mapper_config(),
).unwrap_err();
assert_eq!(err.code(), tonic::Code::InvalidArgument);
```
