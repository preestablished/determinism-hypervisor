# Positive Notes

- The new test uses the public `HypervisorWorker` gRPC service and client over UDS rather than calling worker internals directly.
- The snapstore path is real and in-process, so the restore and snapshot lifecycle exercises the same storage seam used by the worker service.
- The test establishes a single-slot baseline through the same public API path used by the 64-slot run.
- The `ListSlots` assertion after `restore_all` is a useful check that all 64 slots are concurrently occupied before injection and execution proceed.
- The `CaptureSpec` check validates exact extracted framebuffer bytes while keeping full framebuffer capture disabled, which gives a focused API-level assertion.
- The digest helper length-prefixes variable bytes and includes restore, run, snapshot, input-log, config-hash, capture, and frame-counter fields, making accidental equality less likely.
