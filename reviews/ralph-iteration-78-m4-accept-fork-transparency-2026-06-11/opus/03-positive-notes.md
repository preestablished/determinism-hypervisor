# Positive notes

## The two acceptance halves are both honest and additive

- Leg 1 reuses the proven 7c8 H1 == H2 scaffold verbatim (same control, same
  epoch grid, same `assert_eq!(r2, c2)` full-outcome equality) and swaps only the
  middle detour from store-roundtrip to tier-A CoW fork. This is exactly the
  right way to extend an acceptance: the control and the gate are unchanged, so a
  pass attributes the invisibility specifically to the fork, nothing else.

- Leg 2 is materially stronger than the existing static fork tests. Where
  `fork_engine.rs::second_child_sees_the_pristine_parent_after_first_child_diverged`
  compares only RAM + vCPU at the fork point with no execution, leg 2 drives the
  full run loop with injected inputs and reads back the GUEST-VISIBLE ISR table —
  proving the reproducibility property at the level that actually matters
  (identical replay of inputs through delivery), not just at the byte level.

## The equality assertions are load-bearing, not tautological

Both legs assert full `SegmentOutcome` equality, whose `state_hash` is a
full-RAM-walk + canonical-vCPU-blob chain value. A broken replay (any RAM byte,
any non-TSC vCPU field, any icount drift, any mis-delivered injection) would
break the hash. Leg 2 additionally cross-checks the hash against an INDEPENDENT
guest-observable signal — the ISR table written by the guest's own handlers —
which guards against the (hypothetical) case where the host-side hash agreed but
the guest never actually ran the ISRs.

## Non-vacuity is explicitly pinned

`assert_eq!(out_a.injections_delivered, 3)` and
`assert_eq!(vec_a, vec![0x40, 0x41, 0x40])` ensure the test cannot pass with zero
injections delivered or a guest that silently dropped them — the classic
false-pass for an injection replay test. The exact `[0x40, 0x41, 0x40]` ordering
(repeating 0x40) also exercises that the schedule order, not just the vector set,
replays.

## The counter-axis convention is used correctly and consistently

Leg 1's `counter: None` (continuous shared axis, matching the restore control)
and leg 2's `counter: Some(&counter)` (the §3.1 reset, giving each child a clean
0-based agenda for X) are both correct for their respective properties, and the
inline comments point back to the module doc and §3.1 rather than re-litigating
the convention.

## Harness parametrization is clean and mechanical

`boot(elf)` returning the owning `KvmSystem` (so the `sys` lives long enough for
forks) and `run_more(..., injections)` are minimal, well-scoped changes; the
three existing tests are updated by signature only with no behavioral drift, and
the `#[allow(clippy::type_complexity)]` on the 5-tuple return is the pragmatic
call for a test harness.

## Good engineering instinct on the inherited cmdline

Reusing `ITERS_CMDLINE` for the timer guest's boot looks risky at a glance but is
in fact safe — the timer guest only reads the first cmdline byte. The author
correctly relied on this rather than special-casing the cmdline per guest, which
keeps `boot()` uniform. (Worth the one-line comment in S1-adjacent territory, but
the behavior is correct.)
