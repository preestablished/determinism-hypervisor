# M4 ACCEPT — Snapshot Transparency: Review Overview

- **Branch:** `ralph/iteration-75-m4-accept-transparency`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Commit:** `ac04f8b` (ralph: iteration 75 checkpoint — M4 ACCEPT snapshot transparency (H1 == H2))
- **Stats:** 3 files, +286/-0, 1 commit
  - `crates/dh-worker/tests/m4_transparency.rs` (new, +276)
  - `crates/dh-worker/Cargo.toml` (dev-deps, +7)
  - `Cargo.lock` (+3)

## Summary

This is the M4 milestone acceptance gate (bead 7c8): one live integration test that drives
the landing-loop nanokernel guest through a real snapshot/destroy/restore round-trip against
an in-process snapstore-server and asserts the round-trip is invisible to the §8.5 state-hash
chain (H1 == H2). The test is **honest**: the two legs are genuinely independent (the control
leg's slot/chain are fully constructed and dropped before the round-trip leg boots a *fresh*
slot and a *fresh* `StateHashChain::new`), and the only values that cross the boundary are the
control-leg outcomes used in equality assertions. The pre-snapshot `r1 == c1` assertion (a full
`SegmentOutcome` equality including the chain value) closes the tautology gap that would
otherwise make an H1/H2 match ambiguous, and the documented `counter: None` choice is sound: the
shared, never-reset `InstRetired` keeps both legs' agendas and hash-link icounts on the same
absolute (50M) grid, and `run_segment`'s `start_icount == counter.read()` invariant doubles as a
machine-checked proof that the snapshot+destroy+restore detour retired zero guest instructions.
The epoch arithmetic is correct (HALF=1e8 and FULL=2e8 both sit on the 50M grid; 30M iters × 8 =
2.4e8 capacity rules out the completion HLT). The hash chain over a 1:1 clock makes vns a pure
function of icount, so `r2.vns == c2.vns` and `r2.boundary == c2.boundary` (rcx included) hold by
construction and serve as cheap failure-localizers.

The one substantive caveat — not a defect, a scope statement worth recording in the module doc —
is that **device-state transparency is exercised but not *verified*** by this guest: the landing
loop never touches pv-clock/pv-entropy MMIO (any such exit would trip the `on_exit` Err arm), and
`push_final_link` hashes `&[]` device sections, so the entropy/clock state captured and restored
never enters H1/H2. The plan's separate ENTR golden test owns that axis, and the module doc's
transparency claim ("device-state leak shows here") is true only for devices the guest reads. A
handful of cheap assertions (`pages_loaded`, `epoch_index`, a full `r2 == c2`) would tighten the
gate at no runtime cost.

## Verdict

**APPROVE** — The acceptance test is correct, honest, and gates the right property. Findings are
non-blocking: one Important-tier documentation precision fix (the device-state claim) and several
cheap assertion-strengthening suggestions. Nothing here should hold the milestone.
