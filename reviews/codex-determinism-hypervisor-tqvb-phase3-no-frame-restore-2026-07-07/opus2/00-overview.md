# Review Overview — 2nd reviewer

- **Branch:** `codex/determinism-hypervisor-tqvb-phase3-no-frame-restore` vs `main`
- **Date:** 2026-07-07
- **Reviewer:** Claude Opus (2nd reviewer)
- **Stats:** 42 files, +4978 / -401, 11 commits

## Summary

This branch lands the play-60fps M1–M3 work: release-build ops posture plus a
`GetWorkerInfo.build_profile` field (M1); the server-streaming
`RunWithFrameCapture` RPC built on a per-frame `FrameSink` at the FRAME_MARK
`MmioWrite` exit in `runctl.rs`, with `Run`'s actor body extracted into a shared
`drive_recorded_run()` (M2); and live `InjectInputs` during a streaming run via
the `SlotLiveInputs` side channel that bypasses the busy actor command channel
(M3). The core design is sound and the hard constraints hold: the state-hash
chain definition and link points are untouched (links stay on the epoch grid +
final stop, never per frame), capture is read-only and neutral by construction,
backpressure blocking is confined to the slot's dedicated actor OS thread
(`drive_recorded_run` runs there via `with_runtime_mut`), nothing depends on
dh-worker, and no operator-private paths leak into committed files. The
capture-neutrality, cancel-landing, and live-inject replay claims are backed by
dedicated M9 acceptance tests, and the runctl-level frame-sink and live-input
semantics have solid host-runnable and KVM-live unit coverage.

The one issue worth a decision before merge is the **terminal `blocking_send`**
in the streaming handler: after the stalled-consumer watchdog fires and frees
the vCPU / actor thread / pinned core, the terminal `Done` is delivered with an
unbounded `blocking_send` on a channel that a never-reading consumer keeps full
— leaking the `dh-frames-N` thread for exactly the adversarial case the watchdog
exists to defend against. A few smaller items (a replay final-link condition
that was rewritten to read the counter, a gauge that can strand on an
actor-thread panic, and workload-drift brittleness in the M9 budget constants)
are worth a look but do not block.

## Verdict

**NEEDS_DISCUSSION** — no Critical findings; one Important issue (terminal
`blocking_send` partially defeats the watchdog) that should be resolved or
consciously accepted, plus a replay-path condition change worth a targeted test.
