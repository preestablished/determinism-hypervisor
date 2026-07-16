# Critical & Important findings

## Critical

**None.**

## Important

**None blocking.** The items below were investigated as candidate Criticals and CLEARED
by execution; recorded here as the basis for the APPROVE.

### (CLEARED) Blast radius of re-arming guest_debug after every handled exit

`land_at` now calls `set_singlestep(&mut guard, true)` after every non-Debug handled exit
in the near phase, and `step_one_entry` does the same on its Ok-handled exit path. The
concern: `KVM_SET_GUEST_DEBUG` could have side effects beyond TF (clearing a pending #DB,
dr7 churn, exception-bitmap reshuffle) that perturb previously-passing deterministic paths.

- **Verified by execution.** Re-ran the full battery WITH the fix: regression.rs (1e9 twice),
  timer_determinism (102s), if0_deferral (32s), landing_precision (67s), m1_acceptance — ALL
  pass; full workspace 209/0. The injection-chaining path (run_segment → step_one_entry, the
  most side-effect-sensitive consumer) is exercised live by timer_determinism and if0_deferral
  and is bit-stable. No perturbation observed.
- The re-arm only sets the same control word the engine already set at near-approach entry
  (`KVM_GUESTDBG_ENABLE | KVM_GUESTDBG_SINGLESTEP`, all other fields zeroed). It does not touch
  dr7 (left 0) and does not enable hardware breakpoints. On the surviving-trap paths it is a
  no-op-equivalent (re-asserts an already-pending TF); on the eaten-trap path it re-arms TF.

### (CLEARED) The trap-eating premise itself

Independently reproduced with a scratch probe (raw KVM, single-step armed once, never
re-armed). MMIO WRITE eats the trap (free-ran 991 instructions to the next exit); MMIO READ
and PIO OUT keep it (next Debug after 0 instructions). Premise CONFIRMED — see 00-overview.
The regression test fails with the fix reverted (`Overshoot counted 1003`) and passes with it
restored, proving the guard is real, not a tautology.

### (CLEARED) Pending-#DB double-count after the sentinel

The probe showed the MMIO-write trap is *fully* eaten — there is NO deferred #DB that fires
on the next entry. So the step after the sentinel classifies cleanly (the attribution test's
exact per-class counts — 996 plain, 1 cpuid, 1 mmio_read, 1 mmio_write, 1 rep_retire, 255
rep_frozen, region sum 997 — hold, which would be impossible if a phantom Debug were
double-counted). The synthesized boundary (`counter.read()` + `get_regs()`) is taken at the
same machine state a real trap would yield: the write has completed, RIP has advanced, zero
retirement. Sound.

### (CLEARED) step_one_entry "one entry can span the write plus its successor"

The new comment notes that an MMIO write inside a `step_one_entry` call makes that entry span
the write plus its next instruction. For the production consumer (run_segment injection
chaining, which wants exactly-one-queued-vector delivery per entry) this is benign: the entry
is explicitly documented as "NOT one retirement," icount still reflects true retirements, and
the returned boundary is a valid instruction boundary. The timer/if0 tests cover this live and
pass. Documented tradeoff, not a defect.
