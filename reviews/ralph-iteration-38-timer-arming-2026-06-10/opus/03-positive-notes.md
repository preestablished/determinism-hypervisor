# Positive Notes

## P1 — Clean separation of conversion from delivery

`timer_to_injection` is a small pure function that turns a `(TimerArm, ClockRatio,
start_icount)` into a `ScheduledInjection`, with the §4 ceil rule and the §3.4 clamp in
one place. It is host-testable without KVM, and the live path simply reuses it. This is
the right factoring: the deterministic math is isolated and exhaustively unit-tested
(1:1, 2:1 ceil, exact multiple, start-clamp), while the live test only has to prove the
plumbing.

## P2 — The append-then-tag pattern is the minimal correct merge

Pushing the timer onto a clone of `seg.injections` and remembering its slot index
(`Some(all_injections.len() - 1)`) is the simplest construction that (a) reuses the
existing agenda compile unchanged, (b) preserves the ORDER CONTRACT (timer is always the
max index → deterministic tie-break at a shared boundary), and (c) lets delivery identify
the timer by a single `==` against the slot. No new ordering rules, no sort, no special
case in `compile`.

## P3 — Every finish path threaded — no orphaned exit

All four terminal paths (goal, budget/hardcap, paused, halted) and all four
`finish_halted` call sites carry `timer_fired`. It would have been easy to thread the
common budget path and forget the halt or pause exits; the diff threads every one,
including the roll-forward Pause path that constructs `SegmentOutcome` inline. This is the
kind of completeness that prevents "timer fired but the outcome says it didn't" replay
divergences.

## P4 — Doc comments tie code to normative sections precisely

`TimerArm`, `timer_to_injection`, and `TimerFired` each cite the exact ARCH section
(§4 ceil, §3.4 deferral, AUX `TIMER_FIRE` fields) and the `clock.rs` armed/disarm
contract, including the subtle absolute-vs-segment-relative vns distinction (caller
subtracts `vns_base`). A future reader does not have to reverse-engineer which spec rule
each line implements.

## P5 — The live test is honest about what it proves and what it defers

`armed_timer_fires_and_reports_live` documents the budget == deadline merge explicitly —
the queued vector never enters the empty IDT, the outcome returns cleanly, and the
comment states that IDT-equipped *delivery observation* is deferred to bead 583's guest.
It asserts the queue boundary (`delivered_icount == DEADLINE`, no deferral because the
landing loop's own stepping refreshed the IF summary), the vector, the armed deadline,
and `injections_delivered == 1`. It tests the chain that this iteration owns and does not
overclaim end-to-end interrupt handling.

## P6 — Determinism held across 3 live runs

The live timer test and the whole `dh-vmm` suite produced byte-identical results across
three consecutive runs (70 passed each, same icounts). For a PMU-counter / single-step /
KVM-injection path this is the property that matters most, and it held without flakes.
