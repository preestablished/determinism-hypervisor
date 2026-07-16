# Positive Notes

- **The iter-35 demand is actually proven, not asserted.**
  `two_vectors_at_one_boundary_both_deliver_live` schedules 0x40 and 0x41 at the
  same icount, reads the guest's own ISR table back, and asserts `count==2`,
  `vecs==[0x40,0x41]` (order), *and* runs the whole thing twice asserting
  bit-identical (rip, state_hash, vecs). That is the strongest possible form of
  the coverage demand: the guest itself is the witness, and determinism is part
  of the same test.

- **The GDT-before-IDT discovery is documented at the point of pain.** The asm
  comment explains *why* the loader's segment caches alone triple-faulted and
  why the in-memory GDT is required for CS reload on delivery. This is exactly
  the kind of hard-won, non-obvious fact that belongs inline.

- **`step_one_entry` is a clean, minimal abstraction of a subtle fact.** Rather
  than hacking `land_at(+1)` and fighting the overshoot, the change names the
  real semantics (one *entry*, suppressed step over delivery), drops single-step
  on every exit path including errors (R10-safe), and reads the counter only
  after stepping off. The hazard it introduces (NEAR landings inside a delivery
  window) is documented and assigned an owner (M6).

- **The budget fix is principled, not a magic number in the dark.** The constant
  carries a comment tying it to §3.4 (deterministic + loud) and to the concrete
  failure mode (an epoch-sized budget single-steps for minutes). The masked test
  exercises the exact path and asserts both the error class and zero deliveries.

- **Drift is locked down.** `TIMER_GUEST_TABLE_GPA` has a dedicated test that
  parses the asm `%define` and compares it to the Rust constant — the host-side
  table reader and the guest-side writer cannot silently diverge.

- **Interrupt-gate (not trap-gate) choice is correct and load-bearing.** Clearing
  IF on entry is what makes the non-reentrant ISR table-write safe; the masked
  variant's `count==0` assertion is what proves IF semantics are honored
  end-to-end.
