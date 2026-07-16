# Suggestions

## Suggestion: cap retained guest-event payload memory

File: `crates/dh-worker/src/runtime.rs:424`

Rationale: `SlotRuntime.guest_events` is an unbounded `Vec<DrainedGuestEvent>`. A guest that emits many events, or a client that repeatedly filters only one stream while leaving other streams retained, can grow worker memory without a clear limit. The DHILOG keeps SDK_EVENT digests, but `StreamGuestEvents` keeps full payloads in RAM.

Suggested snippet:

```rust
const MAX_RETAINED_GUEST_EVENT_BYTES: usize = 16 * 1024 * 1024;

fn extend_guest_events_bounded(
    runtime: &mut SlotRuntime,
    events: Vec<DrainedGuestEvent>,
) -> Result<(), Status> {
    let current: usize = runtime.guest_events.iter().map(|e| e.payload.len()).sum();
    let incoming: usize = events.iter().map(|e| e.payload.len()).sum();
    if current.saturating_add(incoming) > MAX_RETAINED_GUEST_EVENT_BYTES {
        return Err(Status::resource_exhausted("retained guest events exceed worker limit"));
    }
    runtime.guest_events.extend(events);
    Ok(())
}
```

## Suggestion: document and test cancellation semantics for `StreamGuestEvents`

File: `crates/dh-worker/src/service.rs:3621`

Rationale: The implementation drains selected events from `runtime.guest_events` before returning the server stream. If the client disconnects before reading all yielded items, those selected events are lost. That may be acceptable at-most-once behavior, but it should be explicit because unselected events are retained carefully and clients may infer stronger delivery semantics.

Suggested snippet:

```rust
// StreamGuestEvents is at-most-once: selected events are consumed before the
// response stream is handed to tonic. Unselected events remain retained for
// later filtered calls.
```

Add a unit test that filters two streams, verifies the unselected stream remains available, and records the intended cancellation behavior.

## Suggestion: avoid repeated linear scans of requested stream filters

File: `crates/dh-worker/src/service.rs:3622`

Rationale: `streams.contains(&event.stream)` is fine for tiny filters, but this is easy to make linear in both retained event count and requested stream count. A small set also de-duplicates repeated filter values.

Suggested snippet:

```rust
let want_all = streams.is_empty();
let wanted: std::collections::HashSet<u32> = streams.into_iter().collect();
for event in runtime.guest_events.drain(..) {
    if want_all || wanted.contains(&event.stream) {
        // selected
    } else {
        // retained
    }
}
```
