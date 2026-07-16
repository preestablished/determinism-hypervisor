# Critical Issues

No critical issues found.

# Important Issues

## Important: `GetFramebuffer` violates its response shape for raw framebuffer regions

File: `crates/dh-worker/src/service.rs:2103`

Problem: `read_framebuffer_from_bus` reads a framebuffer region and returns the region bytes, but it hard-codes `width = 0`, `height = 0`, and `stride = 0` for a non-empty `pixels` buffer. The proto contract says `GetFramebufferResponse.pixels` is `stride*height` bytes, so the current response is internally inconsistent. The acceptance fixture explicitly has raw framebuffer bytes and no `FbInfo` descriptor, which makes this a real edge case rather than a simple test oversight: callers cannot distinguish "raw bytes with unknown geometry" from a zero-sized framebuffer except by ignoring the metadata contract.

Suggested fix snippet:

```rust
let fb_info = read_framebuffer_info(channel, &manifest, name)?.ok_or_else(|| {
    Status::failed_precondition(
        "GetFramebuffer requires framebuffer metadata; use ReadGuestMemory.region_ranges for raw framebuffer regions",
    )
})?;
let expected = u64::from(fb_info.stride)
    .checked_mul(u64::from(fb_info.height))
    .ok_or_else(|| Status::failed_precondition("framebuffer dimensions overflow"))?;
if expected != region.len {
    return Err(Status::failed_precondition(format!(
        "framebuffer metadata expects {expected} bytes but region has {}",
        region.len
    )));
}
Ok((fb_info.width, fb_info.height, fb_info.stride, fb_info.format, pixels))
```

If raw framebuffer responses are intentional for this iteration, change the API contract and tests to state that `PF_UNSPECIFIED` permits unknown geometry and that clients must treat `pixels` as an opaque byte region.

## Important: paused introspection validates state before actor serialization

File: `crates/dh-worker/src/service.rs:3458`

Problem: `ReadGuestMemory`, `GetFramebuffer`, and `StreamGuestEvents` call `ensure_paused_slot` before `with_runtime_mut` queues work onto the per-slot actor. A racing `Run` can pass its own write checkout and reach the actor first; the introspection request then executes after the run completes and observes a later paused boundary than the one it validated. The response does report the runtime's current `icount`, but the method-level precondition is not checked at the serialized point where memory, framebuffer, and event queue state are actually read. This is a hidden single-orchestrator assumption and will become brittle under concurrent callers, retries, or TTL-enabled lease policies.

Suggested fix snippet:

```rust
manager
    .validate(&lease, lease_now_ms())
    .map_err(slot_error_to_status)?;
let expected = manager
    .slot_info(lease.slot_id)
    .map_err(slot_error_to_status)?;
if expected.state != dh_vmm::SlotState::Paused {
    return Err(Status::failed_precondition(format!(
        "{method} requires Paused slot, got {:?}",
        expected.state
    )));
}

with_runtime_mut(runtimes.as_ref(), lease.slot_id, move |runtime| {
    ensure_paused_slot(&manager, &lease, method)?;
    if runtime.position.cumulative_icount != expected.icount {
        return Err(Status::aborted(format!(
            "{method} boundary changed from {} to {} before execution",
            expected.icount, runtime.position.cumulative_icount
        )));
    }
    f(runtime)
})?
```

For a stronger fix, add a SlotManager read/introspection reservation or per-slot operation gate so `Run` cannot overtake an already accepted paused-boundary introspection call.
