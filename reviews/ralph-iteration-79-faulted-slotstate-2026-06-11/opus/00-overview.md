# Review — iteration 79: add `Faulted` to dh-vmm SlotState

- **Branch:** `ralph/iteration-79-faulted-slotstate`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** determinism-hypervisor-324 — Add `Faulted` to the dh-vmm `SlotState` machine + transition edges
- **Stats:** 1 file, +46/-1 (107 diff lines), 1 commit (`1d312be`)

## Summary

This change adds a `Faulted` terminal state to dh-vmm's pure `SlotState`
machine (`crates/dh-vmm/src/lib.rs`), closing the gap flagged in the iter-66
review: the proto surface already had `SlotState::FAULTED_S = 5` and
`StopReason::FAULTED = 7`, but the Rust state machine that the slot table
(bead ol1), CoW fork (9e4), and snapshot engine consume had no fault state.

The edge set is minimal and fail-closed:

- `Running → Faulted` — contract violation discovered mid-run.
- `Paused → Faulted` — faults detected at a boundary (the engine pauses first
  whenever it can, so this edge carries most faults).
- `Faulted → Empty` — the **only** exit (DestroyVm); restore lands in a fresh
  slot, never resurrects the faulted one.

`ensure_write_path` gains `FaultedWriteDenied { api }`, so a faulted slot is
write-denied identically to Frozen/Empty (the R9 loud-denial pattern). The
`InvalidTransition` matches-list approach means no `_ =>` arm silently absorbs
new edges, and the exhaustive 5×5 test matrix + `faulted_is_terminal_short_of_destroy`
pin the relation precisely. Self-transitions remain rejected.

Build, `cargo clippy -p dh-vmm`, and the 6 `slot_state_tests` all pass clean.

## Quality of the change

The diff is exemplary for this codebase's style: every edge (and every
deliberate non-edge) is justified in the doc comment with a spec citation, the
tests assert the relation exhaustively rather than spot-checking, and the new
state slots into the existing closed-`match` `ensure_write_path` so the
compiler would have caught a missing arm. The `Frozen → Faulted` exclusion is
reasoned explicitly rather than left implicit.

There are no correctness defects in what was changed. The findings below are
all about what was *not* wired and what is *not* pinned — the two follow-up
gaps that this change surfaces but does not (and arguably should not) close in
the same commit. Neither blocks merge: this is a pure, well-tested state-machine
addition with no integration callers yet.

## Verdict

**APPROVE**

The change is correct, well-tested, and fail-closed. The two gaps it surfaces
(no `StopReason::Faulted` *producer* in the Rust runctl enum to drive the new
edges; no `dh-vmm SlotState ↔ proto SlotState` mirror pin) are real and worth
tracking as beads, but they are follow-up integration/test work that this
state-machine-only commit deliberately does not include. The integration note
already commits the future call sites to adopting the guard.
