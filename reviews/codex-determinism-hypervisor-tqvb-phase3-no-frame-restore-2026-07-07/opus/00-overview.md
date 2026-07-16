# Review Overview

- **Branch:** `codex/determinism-hypervisor-tqvb-phase3-no-frame-restore`
- **Compared to:** `main`
- **Date:** 2026-07-07
- **Reviewer:** Claude Opus

## Summary

This branch decouples the interactive play frame path from per-frame chain
links. It (1) switches M1 ops runbooks to release builds and adds a
`build_profile` field to `GetWorkerInfo`; (2) implements the
`RunWithFrameCapture` server-streaming RPC by extracting `run()`'s actor body
into a shared `drive_recorded_run()` and adding a per-frame `FrameSink`
(`crates/dh-vmm/src/runctl.rs`) called at the `FRAME_MARK` MMIO exit, wired to a
bounded (capacity 2) `tokio::mpsc` whose full state IS the backpressure hold,
with a 30s stalled-consumer watchdog and cancel landing the slot `Paused` at the
next frame boundary; (3) adds an `M3` live-`InjectInputs` side channel
(`SlotLiveInputs`) that bypasses the busy actor command channel and is drained
at each `FRAME_MARK`, DHILOG-logged exactly like pre-scheduled inputs; (4) adds
acceptance and perf-smoke tests plus vmm-level unit tests; and (5) folds in an
earlier detchannel frame-boundary drain fix and a nanokernel `detchannel_frames`
fixture. The state-hash chain definition is untouched: links remain on the
epoch grid plus the final stop, the capture sink is read-only by contract, and
live inputs are logged so replay reproduces the run bit-for-bit. The design
respects every stated hard constraint I could verify: nothing below dh-worker
depends on it, frame blocking happens on the slot's dedicated OS actor thread
(the closure is dispatched through `SlotActor::with_runtime_mut` onto the
`dh-slot-{id}` thread), and the producer never uses `.send().await` (it uses a
`try_send` loop), avoiding the classic bounded-channel pre-fill deadlock.

## Verdict

**APPROVE** — with two Important follow-ups (a terminal-`Done` send that can
still block a detached orchestration thread after the watchdog, and a
capture-neutrality coverage gap in default CI). No Critical findings: the
determinism/chain invariants hold.

## Stats

- 42 files changed, +4978 / -401
- 11 commits
</content>
</invoke>
