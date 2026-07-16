# Findings

## F1 - Critical: DetChannel recordings are not replayable

- Files:
  - `crates/dh-worker/src/service.rs:1266`
  - `crates/dh-worker/src/service.rs:2402`
  - `crates/dh-worker/src/replay_engine.rs:198`
- Problem:
  The checkpoint registers a real `DetChannelDevice` in the worker bus and records live detcall PIO exits through `service_exit_with_detchannel`. Those exits produce canonical DetChannel records such as `DEV_EVENT/PIO_ANSWER`, `RING_PUSH`, and `CONS_BUMP`.

  Replay still drives guest exits through `DeviceRail::service_exit`, which only handles debug-serial PIO and MMIO. A replay of a DetChannel-enabled guest will hit the same `0xD370..0xD39f` PIO exits, but the replay on-exit path treats them as unexpected exits. Existing replay tests only manually apply a DetChannel `DEV_EVENT`; they do not replay a guest that actually performs DetChannel PIO.

- Why this blocks merge:
  This breaks the core record/replay property for the exact guest class introduced by this checkpoint. Capture fixture logs can be recorded but are not covered by VerifyReplay.

- Suggested fix:
  Make replay use a DetChannel-aware exit service that is behaviorally identical to recording, preferably by factoring the worker's DetChannel exit handling into a shared helper used by both `Run` and replay. Then add a hardware-gated regression that records a DetChannel/capture fixture segment, seals it, and verifies `VerifyReplay` reaches `Done`.

## F2 - Important: Capture output allocations are unbounded

- File: `crates/dh-worker/src/service.rs:1465`
- Related lines: `crates/dh-worker/src/service.rs:1506`, `crates/dh-worker/src/service.rs:1532`
- Problem:
  `capture_at_boundary` trusts request lengths and guest-published manifest lengths before allocation. `feature_bytes` capacity is the sum of all requested `len` fields, and each range resizes the output before `read_region` can reject bad extent coverage. Framebuffer capture allocates `region.len` bytes directly from the manifest.

  A bad or hostile guest manifest can advertise a huge framebuffer length, and a client can request very large feature ranges against that manifest. That can force multi-GB allocations or OOM the worker before the manifest/extent read fails.

- Suggested fix:
  Add explicit capture caps before allocating, for example `MAX_CAPTURE_FEATURE_BYTES` and `MAX_CAPTURE_FRAMEBUFFER_BYTES`, and validate both per-range and aggregate output sizes. Reject over-cap requests with `INVALID_ARGUMENT` or `FAILED_PRECONDITION` consistently, then add tests for oversized feature ranges and oversized framebuffer manifest length.

## F3 - Important: `Run` can commit execution and then return a capture error

- File: `crates/dh-worker/src/service.rs:2472`
- Related line: `crates/dh-worker/src/service.rs:2486`
- Problem:
  On a successful guest run, the service marks the slot paused, publishes the new position, and updates runtime state before calling `capture_at_boundary`. If capture validation then fails, for example due to layout-version mismatch or missing region, the RPC returns an error with no `RunResponse`, but the slot has advanced and the active DHILOG has recorded the run.

- Why this matters:
  A caller can observe `FAILED_PRECONDITION` and reasonably retry, causing an accidental second run from a later boundary. This is especially sharp for `layout_version` mismatch, which the bead explicitly requires to surface as `FAILED_PRECONDITION`.

- Suggested fix:
  Define transactional semantics for `Run + CaptureSpec` and test them. The safest API behavior is to avoid returning a bare gRPC error after committing guest execution. If post-run capture can fail, the caller needs the committed boundary in-band, or the service needs a preflight mode that rejects invalid capture specs before executing whenever the channel/manifest is already available.
