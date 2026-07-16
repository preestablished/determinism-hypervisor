# Positive Notes

### P1 — Full `SegmentOutcome` equality, not just the hash

Both new tests assert `assert_eq!(r2, c2)` / `assert_eq!(out_a, out_b)` on
the whole `SegmentOutcome` (`#[derive(PartialEq, Eq)]` over `reason`,
`boundary`, `vns`, `state_hash`, `injections_delivered`) rather than only
the 32-byte hash. This catches a divergence in stop reason, boundary tuple,
vns, or delivery count that a hash-only check could in principle mask. Same
discipline the restore leg established — consistently applied.

### P2 — The non-vacuity pins are present and meaningful

`assert_eq!(out_a.injections_delivered, 3)`, `assert_eq!(count_a, 3)`, and
`assert_eq!(vec_a, vec![0x40, 0x41, 0x40])` ensure the replay test isn't
silently comparing two empty/no-op runs. The guest-visible ISR table is
checked against the *expected* vectors, not just A-vs-B — so a world where
injections quietly stopped landing would fail loudly. This is exactly the
"prove the inputs matter" hygiene the research note flags as a common
pitfall, and it is handled.

### P3 — Counter-axis conventions are used correctly and for the right reason

`counter: None` for the fork-roundtrip leg (shared cumulative axis, mirrors
the restore leg) and `counter: Some(&counter)` for the two-children replay
(§3.1 per-segment reset so A and B share an identical 0-based agenda for X).
The choice is deliberate, documented inline, and matches the actual
`apply_dhsnap` step-6 reset semantics. The 0-based vns axis (pure
`icount*num/den`, no `vns_base` term) makes both children's chain links
identical by construction — the test exploits this correctly.

### P4 — The closure factoring is clean and side-effect-free

`run_child` captures `&parent`, `&counter`, `&bus_p`, `&entropy_p`, the
`Copy` `fork_boundary`, and `&inputs_x`, and runs twice with no hidden
mutation: `build_dhsnap` only *reads* the parent's entropy state
(`entropy.state()`), and each child gets its own fresh `bus_c` and `chain`.
Running A then B over the SAME frozen parent — with B forking only after A
diverged — is the correct, and stronger, ordering for proving reproducibility
from a pristine frozen base.

### P5 — The `boot(elf)` generalization is minimal and well-scoped

Threading the ELF through `boot()` and returning the owning `KvmSystem`
(so the fork test can borrow `&sys` for `fork_slot`) is the right minimal
change; the `#[allow(clippy::type_complexity)]` on the 5-tuple return is a
reasonable, honest concession rather than an over-engineered builder. The
restore test's call sites were updated in lockstep with `_sys` bindings,
keeping that test's lifetime semantics intact.
