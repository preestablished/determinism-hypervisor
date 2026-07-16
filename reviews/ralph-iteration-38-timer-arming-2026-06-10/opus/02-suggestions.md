# Suggestions (non-blocking)

## S1 — Document the silent agenda-exclusion of a beyond-segment timer

`timer_to_injection` (crates/dh-vmm/src/runctl.rs:29-46) clamps the converted icount to
`start_icount + 1`. The doc comment explains the ceil rule and the clamp, but does not
state the downstream consequence: when the clamped (budget 0) or genuinely-late
(deadline > budget) icount falls outside `agenda::compile`'s `(start, final]` window,
the timer injection is **silently dropped** and `timer_fired` returns `None` — no error,
no log. That silence is *correct* (the timer is not disarmed, so it stays armed and
fires in a later segment; see 01 Walk B), but it is a non-obvious interaction between two
files. A reader of `timer_to_injection` alone cannot tell that returning `Ok(_)` does not
guarantee the timer fires this segment.

Suggested addition to the `timer_to_injection` doc comment:

> Note: returning `Ok` does not guarantee the timer fires this segment. If the converted
> (or clamped) icount exceeds the segment's final stop, `agenda::compile` excludes it and
> `timer_fired` is `None`. This is correct one-shot semantics: the device is disarmed
> only on fire, so the still-armed deadline is re-derived against the next segment's
> `vns_base` and fires there.

This costs nothing at runtime and closes the only real comprehension gap in the change.

## S2 — Consider a host-side test for the beyond-budget non-fire path

The host-side test (`conversion_follows_the_ceil_rule_and_clamps`) exercises the ceil
math and the start-clamp, and the live test exercises the budget == deadline merge. The
"deadline beyond budget → timer not delivered, stays armed, `timer_fired == None`" path
(01 Walk B) is reasoned-correct but untested. A cheap `run_segment`-level test with
`timer.deadline_vns` past a short `IcountBudget` would lock that semantics in and guard
against a future agenda-window refactor silently changing it. Not blocking — the behavior
falls out of two already-tested components.

## S3 — `finish` argument count

`finish` now takes 7 args and carries `#[allow(clippy::too_many_arguments)]`
(runctl.rs:183). This is a reasonable, honest suppression for a private helper, and I am
*not* recommending it be changed in this iteration. Flagging only as a watch item: the
next field threaded through the finish paths (e.g. a future device-event delivery
record) would make a small `SegmentResult` aggregate struct worthwhile. Defer until a
second such field actually arrives — refactoring for one is premature.
