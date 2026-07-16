# Suggestions (optional, non-blocking)

All items here are documentation / clarity polish. None change behavior or
coverage; the tests are correct as written.

## S1 — Document that leg 2's reported vns is 0-based, not absolute

In `frozen_parent_children_replay_identical_inputs_identically`, the children run
on the reset (§3.1) axis, so `out_a.vns` and the chain links are 0-based even
though `apply_dhsnap` set `PvClock::vns_base` to the parent's absolute 2M. The
test asserts `out_a == out_b`, which holds because BOTH are 0-based — but a future
reader could mistakenly assume `out.vns` reflects the cumulative timeline (2M +
run). A one-line comment near the `run_child` closure, e.g.:

> // NB: these children run the §3.1 reset axis, so out.vns and the chain links
> //     are SEGMENT-RELATIVE (0-based). vns_base (parent's 2M) only feeds
> //     pv-clock MMIO, which this guest never reads — so it cannot leak into the
> //     hash. A == B because both are cleanly 0-based, not because an absolute
> //     value coincidentally matched.

would pre-empt the exact question this review had to chase through `runctl.rs`
and `hash.rs`. (The module doc's "Counter axis note" covers the icount half but
does not spell out the vns half for the reset path.)

## S2 — Name the `outcome.cumulative_icount` expectation in leg 2

Leg 1 asserts `outcome.cumulative_icount == HALF`, pinning the fork-point
position. Leg 2's `run_child` does not assert `outcome.cumulative_icount ==
PARENT_RUN` before running the child. It is implied by `out_a == out_b` and the
`run_more` `landed exactly` assert, but an explicit
`assert_eq!(outcome.cumulative_icount, PARENT_RUN)` inside the closure (or once,
outside) would make the fork-point position a named precondition rather than an
inferred one, matching leg 1's style.

## S3 — Consider asserting the children's pre-run chain equals the parent's

Leg 1 pins `outcome.chain.value() == r1.state_hash` (the child resumes from the
parent's exact pre-fork link). Leg 2 relies on both children forking from the
same `fork_boundary.hash_chain` but never asserts
`outcome.chain.value() == fork_boundary.hash_chain` for either child. This is
guaranteed by `apply_dhsnap` (`chain: StateHashChain::from_value(time.hash_chain)`)
and indirectly covered by `out_a == out_b`, but a single explicit assert in the
closure would make the "both children resume from the SAME link" invariant — the
crux of the reproducibility property — visible at the assertion site rather than
only in the prose doc-comment.

## S4 — Shared ISR-table read helper

The `frozen_parent_children_replay_identical_inputs_identically` closure inlines
the count + vectors read from `TIMER_GUEST_TABLE_GPA`, which is byte-for-byte the
same logic as `runctl.rs::idt_guest_tests::read_table` (and the loader-side
`SlotVm`-based reader). Per the research note ("shared fixture code deduplicated
rather than copy-pasted"), a `read_timer_table(slot: &SlotVm) -> (u64, Vec<u8>)`
helper in `tests/common/mod.rs` would remove the duplication if the timer guest
is reused in future acceptance legs. Low priority — the current inline copy is
small and self-contained.
