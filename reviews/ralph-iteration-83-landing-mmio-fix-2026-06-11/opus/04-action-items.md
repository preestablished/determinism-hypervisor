# Action items

### Critical

_None._

### Important

_None block the merge._ The one item that deserves an explicit decision before closing
the bead:

- **Confirm the `step_one_entry` analysis and record it.** The injection-chain helper
  (`boundary.rs::step_one_entry`, used at `runctl.rs:280`) shares the
  Debug-exit-after-MMIO structure but is NOT broken the same way (it stops at the first
  `Debug`, so a consumed arming cannot cause a free-run — see
  `01-critical-and-important.md`). This review concludes it is correct as written. The
  action is to capture that conclusion in code (action S1 below) so it is not
  re-investigated or wrongly "fixed" next iteration.

### Suggestions

- **S1 — Add a cross-reference comment to `step_one_entry`'s `Debug` arm** explaining why
  it does NOT need the land_at re-arm (one entry only; stops at the first Debug, hardware
  or emulator-delivered; consumed arming is irrelevant). File: `crates/dh-vmm/src/boundary.rs`.
- **S2 — Fix the off-by-one doc count** in `mmio_stepper_elf`'s comment: "19 retirements
  per iteration" → "18" (3 MMIO retire 0; 16 nops + sub + jnz = 18). File:
  `tests/nanokernel/src/lib.rs`. Not asserted anywhere, cosmetic only.
- **S3 — Record the rip=0 vacuous-probe lesson** in `mmio_stepper.asm`'s header (must be
  a real long-mode crt0-entered guest; raw bytes at rip=0 misdecode in real mode and
  never reach the MMIO hole). File: `tests/nanokernel/asm/mmio_stepper.asm`.
- **S4 — Optional hedge** on kernel-version scoping in the fix comment ("measured on 6.8;
  idempotent re-arm is safe on any version"). File: `crates/dh-vmm/src/boundary.rs`.

### Bead 4a3 close-note guidance

bd `determinism-hypervisor-4a3` is `IN_PROGRESS`. The fix + the two new regressions
**discharge the core defect** (single-step walk free-running past target across MMIO
clusters). When closing:

1. State the **corrected root cause**: not the iteration-50 "MMIO-write eats the trap"
   mechanism (that was already handled), but an **emulator-DELIVERED `Debug` exit on
   MMIO completion consuming the `guest_debug` arming**; fixed by re-asserting
   single-step on every `Debug` exit in `land_at`.
2. Note the regressions: `landing_at_4096_across_mmio_clusters_is_exact_live` (the exact
   iteration-82 shape) and `consecutive_landings_across_mmio_clusters_are_exact_live`
   (120 landings marching the cluster). Both verified load-bearing (reverting the fix →
   loud Overshoot).
3. The bead also flags **goal polling / pause roll-forward safety for device-driven
   guests (M5+)**. With the fix, the iteration-82 `entr_golden` **Goal** variant
   (`Until::Goal{poll_period:4096}` on entropy_draw) is now **expected to pass** — that
   overshoot WAS this exact path. The close-note should say so. If practical, re-running
   that Goal variant once as confirmation (it was reverted to HLT-batch boundaries to
   dodge the bug; `entr_golden.rs` header documents this) would let the bead close with
   the end-to-end repro green, not just the unit-level probe. This is the only residual
   thing standing between "discharged at the engine level" (true now) and "discharged at
   the original repro" — worth one confirmation run before closing.
4. Record the not-shipped alternative (immediate_exit completion belt — EINTR pre-empts
   complete_userspace_io on 6.8) so it isn't retried.

Net: the bead is substantively resolved; recommend an explicit `entr_golden` Goal-variant
confirmation run captured in the close-note, then close.
