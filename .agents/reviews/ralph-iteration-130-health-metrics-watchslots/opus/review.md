## Findings

1. **P1 - `WatchSlots` uses `DATA_LOSS` for ordinary stream lag.**  
   `crates/dh-worker/src/service.rs:3415` wraps the broadcast receiver, and `crates/dh-worker/src/service.rs:3420` maps `BroadcastStreamRecvError::Lagged` to `Status::data_loss`. The API reserves `DATA_LOSS` for determinism violations and tells callers to treat it as P0 (`.agents/docs/determinism-hypervisor/API.md:466`). A slow `WatchSlots` client can now trigger a determinism-page-class error even though the slot table is still healthy and can be resynced with `ListSlots`. Use a non-determinism status such as `RESOURCE_EXHAUSTED` or `UNAVAILABLE`, ideally with guidance to resync via `ListSlots`.

2. **P1 - `dh_worker_landing_single_steps_total` is exposed but never recorded.**  
   The metric is rendered from `landing_single_steps_total` at `crates/dh-worker/src/service.rs:339`, but the field declared at `crates/dh-worker/src/service.rs:232` has no writer in the service path. The real single-step loop lives in `crates/dh-vmm/src/boundary.rs:151`, yet the worker does not receive or increment a count from it, so `/metrics` will report zero forever while claiming to expose ARCH §9 landing single-step telemetry. Plumb a landing-step count out of the boundary/run-control path and increment this counter, or do not mark this ARCH series as implemented.

3. **P2 - The PMI skid histogram is a hard-coded empty placeholder.**  
   `crates/dh-worker/src/service.rs:460` renders `dh_pmi_skid_instructions` with only a `+Inf` bucket, `_sum 0`, and `_count 0`. That satisfies a name-only audit but not the ARCH §9 PMI skid histogram category, nor the existing `dh-verify` histogram shape whose `prometheus()` output emits observed buckets (`crates/dh-verify/src/skid.rs:91`). Scrapes cannot alert on skid margin drift because the worker always reports no samples. Wire in the measured/baselined skid histogram, or make this explicitly unavailable instead of a zero-valued success metric.

4. **P2 - The HTTP health/metrics listener can accumulate stuck tasks from idle clients.**  
   `serve_health_metrics` spawns one task per accepted connection at `crates/dh-worker/src/service.rs:727`, and `handle_health_metrics_connection` then awaits `stream.read()` with no timeout at `crates/dh-worker/src/service.rs:742`. Because the default bind is `0.0.0.0:7401`, clients that connect and send nothing can hold tasks indefinitely. Add a short read/write deadline or switch this endpoint to a small HTTP server stack that enforces header timeouts and size limits.

## Notes

I did not modify production files. I did not run the test suite; this review is based on the `main...a213f0c` diff and the surrounding ARCH/API documentation.
