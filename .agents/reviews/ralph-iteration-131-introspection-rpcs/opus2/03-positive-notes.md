# Positive Notes

- `crates/dh-devices/src/detchannel.rs:765` exposes `stream_guest_event_payload` by reusing the same canonical wire re-encoding as `SDK_EVENT` digesting, which reduces the risk that streamed payloads and digest inputs drift apart.

- `crates/dh-worker/src/service.rs:2016` restores `runtime.bus`, `runtime.entropy`, and `runtime.log` after pause-drain handling before propagating the drain result, so moved runtime resources are not stranded on the normal error paths.

- `crates/dh-worker/src/service.rs:3174` drains detchannel events at the successful run boundary before capture, which keeps pause-boundary ring draining aligned with the boundary where the slot is published as paused.

- `crates/dh-worker/src/service.rs:3618` implements the intended filter behavior for `StreamGuestEvents`: an empty filter selects all streams, selected events are emitted, and unselected events are retained for later calls.

- `crates/dh-worker/src/service.rs:1978` adds a total response-size guard for `ReadGuestMemory`, covering both raw GPA ranges and named region ranges instead of only checking each range independently.

- `crates/dh-worker/src/runtime.rs:370` introduces a small explicit `DrainedGuestEvent` DTO, keeping the runtime queue decoupled from `detguest_host::GuestEvent` internals.
