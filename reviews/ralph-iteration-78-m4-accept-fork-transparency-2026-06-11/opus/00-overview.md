# Review: M4 ACCEPT — fork transparency + frozen-parent second-child replay

- **Branch:** `ralph/iteration-78-m4-accept-fork-transparency`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** a6s (M4 ACCEPT, fork transparency)
- **Stats:** 1 file, +178/-13, 1 commit (`a3173eb`)
- **File touched:** `crates/dh-worker/tests/m4_transparency.rs`

## Summary

This change adds the two halves of bead a6s to the existing M4 acceptance file:

1. `fork_roundtrip_is_invisible_to_the_hash_chain` — the same H1 == H2 milestone
   structure as the proven restore-roundtrip test (bead 7c8), but with a tier-A
   CoW fork in the middle of the 2e8-instruction landing-loop run. The control
   leg is the 1e8 + pause + 1e8 pause-leg; the fork leg freezes the parent at
   1e8, forks with `counter: None` (continuous shared axis), runs the CHILD 1e8
   more, and asserts full `SegmentOutcome` equality (`r2 == c2`), plus the
   pre-fork pin (`r1 == c1`) and the fork-point chain pin
   (`outcome.chain.value() == r1.state_hash`).

2. `frozen_parent_children_replay_identical_inputs_identically` — the frozen-base
   reproducibility property on the timer guest. Parent runs 2M and freezes;
   child A forks with `counter: Some(&counter)` (the §3.1 reset axis), runs 2M
   with inputs X (three scheduled vector injections at 500k/1M/1.5M, 0-based);
   child B forks AFTER A diverged and replays the same X. Asserts full
   `SegmentOutcome` equality (`out_a == out_b`), `injections_delivered == 3`, the
   guest ISR table `[0x40, 0x41, 0x40]` (non-vacuity), and table A == table B.

The harness is mechanically parametrized: `boot()` now takes a guest ELF and
returns the owning `KvmSystem`, and `run_more()` takes an injections slice. The
three pre-existing tests were updated to the new signatures with no behavioral
change.

## Verdict

**APPROVE**

Both halves of bead a6s are genuinely and honestly covered. The acceptance is
not weaker than the bead demands: the H1 == H2 control is the pause-leg
(consistent with 7c8's established pattern, and the module doc already explains
why the pause leg is the right control). The counter-axis reasoning is correct
in both tests (verified against `agenda.rs`, `runctl.rs`, and
`restore_engine::apply_dhsnap`). The vns axis after counter reset is correctly
0-based with no absolute-axis leak (verified: the chain link and the normalized
IA32_TSC slot both consume the 0-based, segment-relative vns). The timer guest's
cmdline parsing makes the inherited `b"30000000"` cmdline harmless. The equality
assertions are load-bearing (full-RAM + vCPU-walk state hashes plus
guest-visible ISR tables read from each child's own CoW memory), not trivially
true. No false-pass shared-state path exists between the two child closures.

The findings below are all SUGGESTIONS — documentation/clarity only. Nothing is
Critical or Important.
