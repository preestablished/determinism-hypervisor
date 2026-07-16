# Positive Notes

- `crates/dh-devices/src/detchannel.rs:765` extracts `stream_guest_event_payload` from the existing SDK_EVENT digest path, so the streaming payload and the digest continue to use one canonical detguest-wire encoder instead of parallel encodings.
- `crates/dh-devices/src/detchannel.rs:825` keeps SDK_EVENT digest computation over the post-header payload bytes, including wire padding, which matches the existing deterministic replay comparison contract.
- `crates/dh-worker/src/service.rs:1978` bounds aggregate `ReadGuestMemory` output to 16 MiB across raw GPA and region ranges, with overflow checks before allocating.
- `crates/dh-worker/src/service.rs:1997` centralizes the paused-slot lease/state precondition for the new introspection methods, keeping stale lease and wrong-state failures consistent.
- `crates/dh-worker/src/service.rs:3465` validates raw GPA overflow, region existence, layout version, offset overflow, and region bounds before returning memory chunks.
- `crates/dh-worker/src/runtime.rs:370` adds a simple owned `DrainedGuestEvent` buffer type, which keeps event transport separate from detguest-host's borrowed/owned payload internals.
