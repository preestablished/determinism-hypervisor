# Critical and Important findings

**None.**

The review specifically probed the acceptance honesty, both counter axes, the
vns-continuity question, false-pass risks, and the cmdline-parsing hygiene
concern. Every one of these came back clean. Details of what was verified:

## Acceptance honesty vs bead a6s — both halves covered

- **First half** ("roundtrip test with a tier-A fork in the middle, H1 == H2").
  `fork_roundtrip_is_invisible_to_the_hash_chain` reproduces the 7c8 control
  exactly (`HALF + (FULL-HALF)` with a plain pause, `c1`/`c2`), then runs the
  fork leg and asserts `r2 == c2` (full `SegmentOutcome`, not just the hash) and
  `r2.state_hash == c2.state_hash`. The fork point is pinned two ways:
  `r1 == c1` (pre-fork legs identical) and
  `outcome.chain.value() == r1.state_hash` (the child resumes from the parent's
  exact pre-fork chain link). This is the H1 == H2 the bead names; the pause-leg
  control is the same one 7c8 uses and the module doc already justifies.

- **Second half** ("parent frozen → child A diverges with inputs X → second
  child B re-run with the same X matches A exactly").
  `frozen_parent_children_replay_identical_inputs_identically` does exactly this:
  child A runs X and writes its ISR table; child B forks from the SAME frozen
  parent AFTER A ran, replays X, and `out_a == out_b` plus the two children's
  ISR tables match. This is materially stronger than the existing static
  `fork_engine.rs::second_child_sees_the_pristine_parent_after_first_child_diverged`
  (which only compares RAM/vCPU at the fork point with no live run): leg 2 drives
  the full run loop, the §3.4 injection chain, and observes real guest ISR
  delivery — no redundant re-testing, genuinely additive coverage.

## Counter-axis correctness — both legs sound

- **Leg 1 (`counter: None`).** `apply_dhsnap` only resets the counter when
  `counter` is `Some`; with `None` the single hardware counter keeps running.
  `run_more` reads `counter.read()` as `start` after the fork, so the child's
  `start_icount == HALF` and `run_segment`'s `actual != start_icount` guard
  passes. The chain links land on the continuous 50M/.../200M epoch grid, exactly
  as the control leg. Matches the restore leg's convention.

- **Leg 2 (`counter: Some(&counter)`).** `apply_dhsnap` calls `c.reset()` (line
  349), zeroing the hardware counter. `run_more`'s `start = counter.read() == 0`,
  so `run_segment` runs `start_icount = 0`. `agenda.rs` documents `start_icount`
  and injection icounts as **segment-relative** (icount 0 at segment start), and
  `compile()` keeps injections in `(start, final]` = `(0, 2M]` — so X's absolute
  icounts 500k/1M/1.5M are valid 0-based agenda points. Nothing in
  `fork_slot`/`apply_dhsnap` assumes the counter was already 0 before the reset:
  child A leaves the shared counter at 2M, and B's fork resets it to 0 again with
  no dependence on the prior value. `run_segment`'s start check holds for both
  children (both read 0 after their own reset).

## vns continuity after reset — no absolute-axis leak (the subtle one)

`apply_dhsnap` sets `PvClock::set_vns_base(time.vns)` = the parent's absolute
2M. The concern was whether that absolute base leaks into the chain. It does
not:

- The chain link's `vns` is `clock.vns_from_icount(point.icount)` in
  `run_segment` (lines 312-314, 370-372), where `point.icount` is the **0-based**
  segment-relative icount. So both the `(icount, vns)` tail of each link and the
  reported `SegmentOutcome.vns` are 0-based.
- `push_final_link` calls `canonical_vcpu_blob(&slot.vcpu, vns)` with that same
  0-based `vns`, which fills the normalized IA32_TSC slot — so the vCPU blob's
  TSC is 0-based too, not the 2M base.
- `vns_base` (2M) only surfaces if the guest READS pv-clock MMIO. The timer guest
  in default mode (the mode selected here, see the cmdline note below) does STI +
  busy-spin and never touches pv-clock. So `vns_base` is dead state for this
  guest and cannot leak.

Net: A and B are identical because both run a clean 0-based axis. There is no
absolute-axis value that "happens to be equal anyway" — the absolute base is
simply never consumed by anything the assertions observe. (Worth a one-line test
comment; filed as a Suggestion, not a defect.)

## False-pass risks — none

- `fork_boundary: BoundaryState` derives `Copy` (snapshot_engine.rs:50), so the
  closure copies it per call; no mutation shared between A and B.
- `parent` (`&SlotVm`, frozen CoW base), `bus_p` (`&MmioBus`), `entropy_p`
  (`&DetEntropy`) are captured by shared reference and are read-only inside
  `fork_slot` (`parent`, `parent_bus`, `parent_entropy` params — none mutated).
  The CoW guarantee that B sees the pristine parent after A diverged is the exact
  property `fork_engine.rs` already proves at the RAM level.
- `counter` (`&InstRetired`) is the one mutable-via-hardware shared object, but
  each `run_child` resets it to 0 inside its own `fork_slot`, so there is no
  carry-over from A into B.
- The ISR tables are read from `child.guest_mem` (each child's own CoW mapping),
  not a shared buffer, so `vec_a`/`vec_b` reflect actual per-child ISR execution.
- The chain comparison is meaningful: both children resume from the SAME
  `fork_boundary.hash_chain`, by design, and the full-RAM + vCPU-walk state hash
  would diverge on any replay break.

## Cmdline hygiene — `b"30000000"` is harmless to the timer guest

`ITERS_CMDLINE = b"30000000"` is inherited by `boot()` for the timer guest in
leg 2. Verified against `tests/nanokernel/asm/timer_guest.asm`: the guest
inspects only the **first byte** of the cmdline (`movzx eax, byte
[rsi + BOOTINFO_OFF_CMDLINE]`) and branches on `'m'`/`'a'`/`'d'`. `'3'` (0x33)
matches none, so control falls through to `.open_window` (STI + spin) — exactly
the default mode `timer_determinism.rs` exercises with `b""`. A non-empty,
non-`m/a/d`-leading cmdline produces no behavior change. Not a bug.
