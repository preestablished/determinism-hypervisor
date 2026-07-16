# Suggestions (non-blocking)

## S1 — Cross-reference comment in `step_one_entry` (highest-value follow-up)

`step_one_entry` (`crates/dh-vmm/src/boundary.rs` ~line 224, used by the injection chain
at `runctl.rs:280`) shares the structural ingredient the iteration-83 fix is about: it
single-steps and can cross an emulated MMIO instruction whose completion delivers a
`Debug` exit. It is **correct as written** (it `break`s on the first `Debug`, so a
consumed arming cannot cause a free-run — see `01-critical-and-important.md`), but the
equivalence argument is non-obvious. A future editor "mirroring the land_at fix" could
either add a dead re-arm or, worse, misread the helper.

Add a one-line note to `step_one_entry`'s `Ok(VcpuExit::Debug(_)) => break Ok(())` arm,
e.g.:

```rust
// Unlike land_at's walk, ONE entry only: we STOP at the first Debug
// (hardware OR emulator-delivered, e.g. an MMIO completion — bead 4a3),
// so a consumed single-step arming cannot free-run here; no re-arm needed.
Ok(VcpuExit::Debug(_)) => break Ok(()),
```

This bottles the analysis at the code so it survives across iterations.

## S2 — Fix the "19 retirements per iteration" doc count (off by one)

`tests/nanokernel/src/lib.rs` (the `mmio_stepper_elf` doc comment) states "19
retirements per iteration." The loop body is 3 MMIO instructions (retire 0) + 16 nops +
`sub ecx,1` + `jnz` = **18** retiring instructions per iteration, not 19. Nothing asserts
this number (the regressions land at arbitrary counter offsets and read the real
counter), so it is purely cosmetic — but it is a stated empiric and should be correct.
Change "19" to "18".

## S3 — Record the "vacuous raw-code at rip=0" lesson where it can prevent recurrence

The commit history notes the first probe attempt injected raw 64-bit code at `rip=0`,
which was vacuous (real-mode misdecodes 64-bit encodings and never reaches the MMIO
hole), so the real probe is a proper long-mode ELF guest. That lesson currently lives
only in commit prose. A one-line comment in `mmio_stepper.asm`'s header (it already
explains the cluster shape) noting "must be a real long-mode guest entered via crt0 —
raw bytes at rip=0 misdecode in real mode and never reach the MMIO hole" would stop the
next person from re-introducing a vacuous rip=0 probe. Optional but cheap insurance.

## S4 — Minor hedge on kernel-version scoping in the comment

The fix comment is anchored to "kernel 6.8." The consumed-arming behavior is an emulator
internal that could shift across kernel versions. The comment already correctly notes the
hardware/emulator indistinguishability and that the re-arm is idempotent, so the fix is
version-robust regardless. No change strictly needed; if you want to be explicit, add
"(behavior measured on 6.8; the idempotent re-arm is safe on any version)" so a future
kernel bump doesn't prompt a needless re-investigation.

## Residual coverage gap (informational, not a defect)

**PIO under single-step is exercised but the new probe does not cover it.** The
mmio_stepper probe covers imm-dword-write / reg64-write / dword-read clusters but no PIO.
PIO-under-stepping IS exercised elsewhere:
- `counting_semantics::landing_across_an_mmio_write_does_not_free_run` does a near
  (stepping) landing whose walk crosses the `'S'` serial OUT (PIO, icount 6) and the
  MMIO read/write at icount 12 — and lands exact at 20.
- `counting_semantics::trace_counting` single-steps the whole region including both
  marker OUTs via `step_one_entry`.

PIO exits arrive as `IoOut`/`IoIn`, handled by the `Ok(exit)` arm which already re-arms
(iteration-50 fix) — they are not `Debug` exits, so the new mechanism does not apply to
them, and existing tests confirm PIO-under-stepping lands exact. **No new gap.** If you
want belt-and-suspenders symmetry with the MMIO probe, a PIO variant could be added, but
the existing counting-guest coverage makes it low value. Not recommended as a blocker.
