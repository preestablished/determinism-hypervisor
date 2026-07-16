# Review: wire `Until::NextSdkEvent` + `Until::FrameBudget` (bead 4qo)

- **Branch:** `ralph/iteration-95-runctl-wire-until-nextsdkevent-fram`
- **Base:** `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Stats:** 13 files, +503 / -38, 1 commit (`c2f91e4`)

## What the change does

Wires the two formerly-`NotYetWired` until-modes from API.md §2.4, completing all
five Phase-1 stop conditions in `crates/dh-vmm/src/runctl.rs`:

- **`Until::FrameBudget { frames, hard_cap }`** — runctl itself decodes the
  frame-boundary exit (`VcpuExit::MmioWrite` at `pad::PV_PAD_BASE +
  pad::REG_FRAME_COUNTER`), counts marks in *every* mode (new
  `SegmentOutcome.frames_elapsed`), and stops ON the Nth mark's exit boundary
  with `BudgetReached`. `frames == 0` stops at the start boundary without a
  guest entry (mirrors `IcountBudget(0)`).
- **`Until::NextSdkEvent { hard_cap }`** — caller-fed via the new
  `Segment::sdk_events: Option<&Cell<u64>>`. The device rail applies the
  stream filter and bumps the cell inside `on_exit`; runctl stops ON the
  feeding exit when the count rises above its segment-start baseline. A missing
  feed now errs loudly via the new `RunError::MissingSdkEventFeed` (the old
  `NotYetWired` variant is removed).

Both modes reuse the HLT sentinel-unwind: `on_exit` services the exit FIRST,
then the wrapper sets a flag and returns a sentinel `BoundaryError`; every
flight site catches it via the flag and calls the generalized
`finish_at_counter` (renamed from `finish_halted`). New `StopReason::NextSdkEvent`
is threaded through `recording.rs` (END byte 3), `proto_map.rs` (+wire pins),
and `dh-cli run.rs` (report string). `replay_engine.rs` reseal passes
`frames_elapsed: 0` with a comment explaining `seal()` never reads it.

Five new live KVM tests (`event_until_tests`) pin the semantics: stop-on-Nth-mark,
`frames==0`, hard-cap fallthrough, stop-at-feeding-exit, missing-feed-errs.

## Verdict

**APPROVE**

The change is determinism-correct, the unwind covers every flight site with the
right priority order, the byte/proto mappings are exhaustively cross-pinned, and
it conforms to API.md §2.4. I verified locally: `cargo build`/`clippy` clean on
both crates, all 5 `event_until_tests` pass, run-twice-identity assertions hold,
and the suite is stable across 3 consecutive runs. No Critical or Important
issues. A handful of non-blocking suggestions are in `02-suggestions.md`.

## Verification performed

- `cargo build -p dh-vmm -p dh-worker` — clean.
- `cargo clippy -p dh-vmm -p dh-worker` — no warnings.
- `cargo test -p dh-vmm event_until` — 5/5 pass, repeated 3× (stable).
- `cargo test -p dh-worker --lib proto_map` — 3/3 pass (wire pins + END-byte
  cross-pin + slot-state pins).
- Cross-checked `proto/hypervisor.proto` (source of truth) and the API.md mirror:
  `NEXT_SDK_EVENT = 3`, `BUDGET_REACHED = 1`, `frames_elapsed` semantics, and
  "hard cap `0 ⇒ worker default (10e9)`" all match.
