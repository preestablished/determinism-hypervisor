# Iteration 78 — M4 ACCEPT fork transparency — Review (Opus, 2nd reviewer)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-78-m4-accept-fork-transparency`
- **Scope:** 1 file, +178/-13 — `crates/dh-worker/tests/m4_transparency.rs`
  (bead a6s M4 ACCEPT: fork-roundtrip transparency + frozen-parent
  second-child replay).

## Summary

The diff generalizes the existing `boot()` helper to take an `elf` and
return the owning `KvmSystem`, threads an `injections` slice through
`run_more`, and adds two acceptance tests:

1. `fork_roundtrip_is_invisible_to_the_hash_chain` — the H1==H2 milestone
   with a tier-A CoW fork (counter `None`) standing in for the store
   round-trip.
2. `frozen_parent_children_replay_identical_inputs_identically` — two
   children forked from one frozen timer-guest parent (counter `Some`,
   §3.1 reset), each replaying the same 3-vector input set X; full
   `SegmentOutcome` equality plus a guest-visible ISR-table read-back.

I verified the load-bearing assumptions against the real code rather than
the summary, and all of them hold:

- **Timer guest + cmdline (the riskiest one).** `boot()` now passes
  `ITERS_CMDLINE = b"30000000"` to *every* guest, including the timer
  guest, which previously booted with `b""` (`timer_determinism.rs`) or
  `b"defer"` (`if0_deferral.rs`). I read `tests/nanokernel/asm/timer_guest.asm`:
  the mode select (lines 66–78) tests only the FIRST cmdline byte against
  `'m'`/`'a'`/`'d'`. The first byte of `"30000000"` is `'3'` (0x33), which
  matches none, so it falls through to `.open_window` → `sti` → `.masked`
  spin — **byte-identical control flow to the empty-cmdline path** (which
  `jz/je .open_window`s to the same `sti`). The numeric value is otherwise
  ignored by this guest. No iteration-count change, no parse-failure halt,
  no vacuity break. This is correct but *fragile and undocumented* — see
  01 / 02.

- **vns axis for chain links.** `run_segment` computes every link's vns as
  `seg.config.clock.vns_from_icount(point.icount)` (`runctl.rs:312`), and
  `ClockRatio::vns_from_icount` (`vt.rs:43`) is a pure `icount*num/den` with
  **no `vns_base` term**. So links are 0-based counter-space for both
  children after the §3.1 reset — identical for A and B regardless of
  PvClock's `vns_base`. Assumption holds.

- **counter `Some` reset / race.** The reset is `apply_dhsnap` step 6
  (`restore_engine.rs:347`), strictly before the child runs; everything is
  single-threaded on this thread and the slot is not running between
  `reset()` and the next `run_more`→`counter.read()`. No window to flake.

- **Closure capture / hidden mutation.** `build_dhsnap` takes
  `&DetEntropy` and only calls `entropy.state()` (read-only); the
  `run_child` closure running twice over `&entropy_p` is sound. No
  order-dependence between A and B beyond the intended counter reset.

- **leg-1 `outcome.chain.value() == r1.state_hash`.** Meaningful — proves
  the fork point's chain link round-trips through the in-memory DHSNAP's
  TIME section, exactly as the restore leg's analogous assertion does.

The tests are well-constructed and the assumptions are sound. My findings
are about robustness of the implicit timer-guest contract, a couple of
cheap strengthenings, and naming/maintainability.

## Verdict

**APPROVE.** No correctness defects. One Important item (make the
timer-guest cmdline dependence explicit so a future cmdline change can't
silently flip the guest into `mask`/`arm`/`defer` mode and quietly gut the
test). The rest are Suggestions.

## Stats

| Severity   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 1     |
| Suggestion | 5     |
| Positive   | 5     |
