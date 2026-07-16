# Iteration 79 — `Faulted` SlotState — Review Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-79-faulted-slotstate`
- **Bead:** 324
- **Scope:** 1 file, `crates/dh-vmm/src/lib.rs`, +46/-1

## Summary

The change adds a `Faulted` variant to the `dh-vmm` `SlotState` machine: the
enum gains `Faulted`; `SlotStateError` gains `FaultedWriteDenied { api }`; the
`can_transition` relation gains three edges (`Running→Faulted`,
`Paused→Faulted`, `Faulted→Empty`); `ensure_write_path` denies on `Faulted`;
and the test module is widened to a 5×5 matrix plus a new
`faulted_is_terminal_short_of_destroy` terminality test. `Frozen→Faulted` is
deliberately absent with a documented rationale.

The implementation is correct, fail-closed, and internally consistent. All six
`slot_state` unit tests pass. The interesting findings are not in this file —
they are in the **negative space around it**: the relationship between this
in-memory enum and the proto wire enum, and the revisitability of the
`Frozen→Faulted` omission as a host-side (not guest-side) fault class.

## Consumer sweep (semantic drift)

Every workspace consumer of `SlotState` was inspected. The three engines all
gate on a **positive** equality check (`!= Paused` for snapshot/restore,
`!= Frozen` for fork) and carry the rejected state in a `Debug`-only error
(`NotPaused { state }`, `ParentNotFrozen { state }`). A `Faulted` slot
therefore flows through every gate **fail-closed automatically** — no engine
match needs updating, no error message is now misleading, and crucially **none
of these errors carry any retry-suggesting Display text** (the error enums are
`#[derive(Debug)]` only, no `Display`/`thiserror` impl). This is the right
outcome and the positive-check style is what makes the new variant safe to add
without touching consumers.

## Verdict

**Approve.** No Critical or Important blockers. The implementation is sound and
the tests pass. The findings are forward-looking guardrails (a proto↔domain
mapping that does not yet exist but is now a latent foot-gun) and one design
question worth recording (host-side `Frozen→Faulted`) — both belong in a bead,
not in a blocking change to this diff.

## Stats

| Category    | Count |
|-------------|-------|
| Critical    | 0     |
| Important   | 1     |
| Suggestions | 4     |
| Positive    | 5     |
