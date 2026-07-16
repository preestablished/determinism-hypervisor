# Suggestions

## Suggestion: Use a set for StreamGuestEvents filtering

- File: `crates/dh-worker/src/service.rs:3618`
- Rationale: `streams.contains(&event.stream)` makes filtering `O(events * streams)`. The request list is small today, but this path is already draining a potentially large in-memory event backlog, so the constant-time form is clearer and keeps the implementation predictable.

Suggested snippet:

```rust
let stream_filter: std::collections::HashSet<u32> = streams.into_iter().collect();
let want_all = stream_filter.is_empty();

for event in runtime.guest_events.drain(..) {
    if want_all || stream_filter.contains(&event.stream) {
        selected.push(proto::GuestEvent {
            stream: event.stream,
            icount: event.icount,
            vns: event.vns,
            payload: event.payload,
        });
    } else {
        retained.push(event);
    }
}
```

## Suggestion: Factor framebuffer region lookup/read logic

- File: `crates/dh-worker/src/service.rs:2078`
- Rationale: `read_framebuffer_from_bus` and `capture_at_boundary` both walk the manifest for the first live `FRAMEBUFFER` entry and read the region. Once descriptor parsing is fixed, keeping two copies increases the chance that `GetFramebuffer` and `CaptureSpec.framebuffer` diverge again.

Suggested snippet:

```rust
struct FramebufferRead {
    width: u32,
    height: u32,
    stride: u32,
    format: i32,
    pixels: Vec<u8>,
}

fn read_framebuffer_region(
    channel: &detguest_host::Channel<RuntimeVmMem>,
    manifest: &detguest_host::RegionManifest,
) -> Result<FramebufferRead, Status> {
    // Shared FRAMEBUFFER lookup, descriptor decode, size checks, and pixel read.
}
```

## Suggestion: Add focused regression tests for the risky edges

- File: `crates/dh-worker/src/service.rs:5002`
- Rationale: The new end-to-end test covers the happy path on a zero-base segment, but the risky behavior is in nonzero segment bases, descriptor-bearing framebuffer regions, filtered stream retention, stale leases, non-paused states, and pause-drain failure handling.

Suggested snippet:

```rust
#[test]
fn stream_events_after_restore_use_cumulative_icount_but_segment_log_icount() {
    // Create base snapshot, restore it, emit a detchannel event in the child
    // segment, stream it, then TakeSnapshot and verify DHILOG sealing succeeds
    // with segment-relative END ordering.
}

#[test]
fn get_framebuffer_decodes_descriptor_and_returns_only_pixels() {
    // Publish a FRAMEBUFFER region with a real descriptor plus pixels, then
    // assert width/height/stride/format and pixels.len() == stride * height.
}
```
