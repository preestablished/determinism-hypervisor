# Review Overview — runctl: wire Until::NextSdkEvent + Until::FrameBudget

- **Branch:** `ralph/iteration-95-runctl-wire-until-nextsdkevent-fram` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** 4qo
- **Stats:** 13 files, +503 / -38, 1 commit (`c2f91e4`)
- **Core file:** `crates/dh-vmm/src/runctl.rs`

## Summary

This change finishes the `Until` enum: the two previously-`NotYetWired` event-driven
modes — `FrameBudget { frames, hard_cap }` and `NextSdkEvent { hard_cap }` — are now
live. Both walk toward the request's `hard_cap` (`FinalStop::HardCap`) and stop EARLY
on a triggering exit, unwinding the in-flight boundary engine via the same sentinel-Err
mechanism that terminal HLT already used. Frame marks are decoded directly in the
`exits!` wrapper against `dh_devices::pad` constants (the pv-pad `FRAME_COUNTER` MMIO
write); SDK-event matching is caller-fed through a new `Segment::sdk_events: Option<&Cell<u64>>`,
where the rail bumps a monotone count per matching drained event and runctl stops when
the count rises above its segment-start baseline.

The HLT-specific `finish_halted` is generalized to `finish_at_counter(reason, …)`,
shared by HLT and both event stops. A new `SegmentOutcome.frames_elapsed` field is
counted in EVERY mode and surfaced; `StopReason::NextSdkEvent` (proto byte 3) is threaded
through `recording.rs`, `proto_map.rs`, and `dh-cli`. `RunError::NotYetWired` is removed
and replaced by `RunError::MissingSdkEventFeed` (loud failure when NextSdkEvent runs
without its feed). Live KVM tests cover both modes including the `hard_cap` safety net,
`FrameBudget(0)`, and replay-identical-boundary checks.

## Assessment

The implementation is careful and well-reasoned. I independently verified the riskiest
implicit assumptions:

- **Boundary stability across record/replay** — the triggering exit (FRAME_COUNTER MMIO
  write / doorbell) has NOT retired when `finish_at_counter` reads the counter
  (`boundary.rs` "an instruction that exited mid-emulation has not retired"). This is the
  identical invariant the pre-existing HLT path relied on; the icount is the count of
  instructions before the writing one, deterministic on both legs. Sound.
- **No frame double-counting on retry** — `land_at` / `step_one_entry` propagate an
  `on_exit` `Err` immediately (`?` / `break Err`), with NO retry path that re-services the
  same exit. ARCH §6.6 guarantees "one exit per frame". `frames_seen += 1` fires once per
  exit. Sound.
- **`halted` vs `event_stop` mutual exclusion** — the `Hlt` arm returns before the frame/sdk
  checks; non-HLT exits never set `halted`; the first flag set unwinds the flight. Both
  cannot be true. The `halted`-first ordering is harmless. Sound.
- **`FrameBudget(0)` link consistency** — `IcountBudget(0)` on the epoch grid produces a
  final-stop point with `epoch_hash = false` (the grid loop's first candidate is strictly
  after `start`), so `finish()` pushes exactly one link; `FrameBudget(0)` routes through
  `finish_at_counter` (`already_hashed = false`) and also pushes exactly one link. No
  double-link. Consistent.
- **`frames_elapsed` in derived `Eq`** — the two full-`SegmentOutcome` equality sites
  (`m4_transparency.rs:262,421`, `r1==c1` / `r2==c2`) compare two runs that both use
  `Until::IcountBudget` with an `on_exit` that errors on ANY exit and a guest that never
  writes FRAME_COUNTER, so `frames_elapsed == 0` on both sides — no false divergence.
  `replay_engine.rs` compares fields individually, never the full struct.
- **dh-cli reachability** — dh-cli constructs only `IcountBudget`/`VnsBudget`; it never
  builds `Until::NextSdkEvent`, so `sdk_events: None` cannot trip `MissingSdkEventFeed`
  from the CLI.

I found no Critical or Important defects. A small number of non-blocking suggestions
(maintainability of the repeated 4-arm unwind, a redundant `ok_or` evaluation, a minor
doc-precision nit) appear in `02-suggestions.md`.

## Verdict

**APPROVE.** The change is correct, spec-aligned (API.md §2.4, ARCH §3.3/§6.6), and the
event-stop boundary is replay-deterministic by the same argument the HLT stop already
depended on. Suggestions are quality-of-life only.
