# Positive notes

## P1 — Root cause was re-isolated, not assumed

The original 4a3 hypothesis (the iteration-50 "MMIO-write eats the trap" mechanism) was
*already handled* in the code. Rather than piling on, this iteration used a granular
probe walk to discover the **distinct** mechanism — an emulator-DELIVERED `Debug` exit
on MMIO completion consuming the arming — and the commit explicitly records that the old
hypothesis was wrong ("TF survives the MMIO completions themselves"). Correcting a prior
diagnosis under load is exactly the discipline this determinism-critical engine needs.

## P2 — The fix is minimal, idempotent, and correctly fenced

One `set_singlestep(true)` call in the `Debug` arm, inside the stepping branch only. It
cannot fire on the far (non-stepping) approach, it is a no-op when the trap survived, and
it touches only host-side trap control — never guest state. Minimal blast radius for a
core-loop change.

## P3 — Regressions are strong and demonstrably catch the bug

Two live tests: an exact repro of the iteration-82 shape (single landing at 4096 deep in
the MMIO loop) and a marching test (120 landings, stride `1 + k%23`, hitting every offset
in the cluster body). I verified by experiment that reverting the fix makes BOTH fail
with loud `Overshoot` (4096→4110, 101→114) and restoring it makes both pass — the tests
are real regressions, not tautologies, and the marching test's varied stride genuinely
exercises every step-walk distance against every instruction offset.

## P4 — The probe guest is honest about its purpose and isolation

`mmio_stepper.asm` targets unbacked hole space with the on_exit handler acking writes and
zero-filling reads, deliberately removing any device model so the test isolates the
KVM emulation/trap interaction from device side effects. The header comment ties it
directly to the entropy_draw doorbell cluster it mirrors, and it is wired through the same
build.rs/lib.rs/elf_shape drift-tested path as every other guest (it is now asserted to
be a static x86-64 exec at the load addr).

## P5 — Failed approaches are recorded, not silently dropped

The commit captures two dead ends — the immediate_exit completion belt that does NOT work
on 6.8 (EINTR pre-empts complete_userspace_io) and the vacuous raw-code-at-rip=0 probe.
Recording what was tried and *why it failed* is high-value institutional memory for an
autonomous iteration loop; it prevents the next iteration from re-walking the same blind
alleys. (Suggestions S1/S3 propose moving the most reusable of these lessons into the
code so they survive beyond the commit log.)
