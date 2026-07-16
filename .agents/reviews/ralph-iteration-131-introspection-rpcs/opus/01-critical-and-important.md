# Critical And Important Issues

## Critical: Pause drains stamp DHILOG records with cumulative icounts and stream events with mixed icount domains

- Severity: Critical
- File: `crates/dh-worker/src/service.rs:2033`

`drain_runtime_detchannel_at_pause` uses `runtime.position.cumulative_icount` to build `DevCtx`, so detchannel `CONS_BUMP` and `SDK_EVENT` records logged at a pause boundary are appended to the active per-segment DHILOG with a cumulative icount. After restore/snapshot, `segment_icount` resets to 0 while `cumulative_icount` remains the lineage position; the next `TakeSnapshot` seals with `end_icount = runtime.position.segment_icount`, so any pause-drain record stamped with cumulative icount can make the later `END` record go backwards. The run-exit path has the opposite problem for the external stream: `service_exit_with_detchannel` receives the raw counter value and stores that as `GuestEvent.icount`, so events drained during an exit are segment-relative while events drained at pause are cumulative.

Suggested fix snippet:

```rust
fn cumulative_from_segment(
    start_segment_icount: u64,
    start_cumulative_icount: u64,
    segment_icount: u64,
) -> u64 {
    start_cumulative_icount
        .saturating_add(segment_icount.saturating_sub(start_segment_icount))
}

fn service_exit_with_detchannel(
    rail: &mut dh_vmm::recording::DeviceRail<RuntimeVmMem>,
    log_icount: u64,
    event_icount: u64,
    exit: kvm_ioctls::VcpuExit<'_>,
) -> Result<Vec<DrainedGuestEvent>, dh_vmm::boundary::BoundaryError> {
    let mut ctx = dh_devices::DevCtx::new(
        log_icount,
        0,
        &mut rail.log,
        &mut rail.mem,
        &mut rail.entropy,
        &mut rail.irqs,
    );
    // ...
    drained_guest_events_to_runtime(events, event_icount)
}

let log_icount = counter_ref.read().map_err(|e| {
    dh_vmm::boundary::BoundaryError::Exit(format!("counter read: {e:?}"))
})?;
let event_icount =
    cumulative_from_segment(start_segment_icount, start_cumulative_icount, log_icount);
let events = service_exit_with_detchannel(
    &mut rail.borrow_mut(),
    log_icount,
    event_icount,
    exit,
)?;
```

For pause drains, build `DevCtx` with `runtime.position.segment_icount`, but convert the returned `DrainedGuestEvent.icount` with `runtime.position.cumulative_icount`.

## Important: Pause-drain failures after a successful run leave the slot reusable instead of Faulted

- Severity: Important
- File: `crates/dh-worker/src/service.rs:3174`

The run success branch publishes the slot as `Paused` at the new position before calling `drain_runtime_detchannel_at_pause(runtime)?`. If that drain detects a detchannel anomaly, log fault, or stream re-encoding failure, the RPC returns an error but the manager state remains `Paused` and the runtime thread is already `Parked`. Earlier drain errors inside KVM exits fault the slot through the `run_result` error path, but pause-boundary drain errors do not. That lets clients continue from a slot whose detchannel drain invariant failed and may already have consumed guest ring state or mutated the DHILOG.

Suggested fix snippet:

```rust
if let Err(e) = drain_runtime_detchannel_at_pause(runtime) {
    runtime.thread = RuntimeThreadState::Faulted(format!(
        "detchannel pause drain: {}: {}",
        e.code(),
        e.message()
    ));
    let _ = manager.mark_faulted(lease.slot_id);
    return Err(e);
}
```

Apply the same faulting policy anywhere a pause-boundary drain mutates detchannel state before returning a `DATA_LOSS`-class error.

## Important: GetFramebuffer does not decode or return framebuffer metadata

- Severity: Important
- File: `crates/dh-worker/src/service.rs:2103`

`read_framebuffer_from_bus` finds the `FRAMEBUFFER`-flagged region and reads it, but returns `width = 0`, `height = 0`, `stride = 0`, `PF_UNSPECIFIED`, and the entire region as `pixels`. The API says `GetFramebufferResponse.pixels` is `stride * height` bytes, and the architecture states the framebuffer region starts with a descriptor containing `{width, height, stride, pixel_format}`. Real clients cannot render or validate the frame from all-zero metadata, and a real descriptor-bearing region would be returned with descriptor bytes mixed into `pixels`. The added test uses the current nanokernel fixture, which explicitly has no framebuffer descriptor, so it cannot catch this contract violation.

Suggested fix snippet:

```rust
let mut region_bytes = vec![0u8; fb_len];
channel
    .read_region(name, 0, &mut region_bytes)
    .map_err(|e| capture_region_error(name, e))?;

let desc = FbDescriptor::decode(
    region_bytes
        .get(..FbDescriptor::WIRE_LEN)
        .ok_or_else(|| Status::data_loss("framebuffer descriptor is truncated"))?,
)?;
let pixel_len = desc
    .stride
    .checked_mul(desc.height)
    .and_then(|n| usize::try_from(n).ok())
    .ok_or_else(|| Status::invalid_argument("framebuffer pixel length overflows"))?;
let pixel_start = FbDescriptor::WIRE_LEN;
let pixel_end = pixel_start
    .checked_add(pixel_len)
    .ok_or_else(|| Status::invalid_argument("framebuffer pixel length overflows"))?;
let pixels = region_bytes
    .get(pixel_start..pixel_end)
    .ok_or_else(|| Status::data_loss("framebuffer pixels are truncated"))?
    .to_vec();

Ok((desc.width, desc.height, desc.stride, desc.format.into(), pixels))
```

The descriptor type should come from a shared guest-sdk-owned layout so the worker and producer cannot drift.
