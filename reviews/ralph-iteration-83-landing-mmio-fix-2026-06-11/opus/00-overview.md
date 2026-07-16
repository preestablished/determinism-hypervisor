# Review: iteration-83 landing-vs-MMIO fix (bead 4a3)

- **Branch:** `ralph/iteration-83-landing-mmio-fix`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Scope:** §3.2 landing engine (`crates/dh-vmm/src/boundary.rs`), new nanokernel probe guest `tests/nanokernel/asm/mmio_stepper.asm` + wiring, two new live regressions.
- **Diff:** 5 files, +152/-1.

## Summary

The change fixes the iteration-82 landing overshoot (bead 4a3): inside MMIO-dense
guest code, `land_at`'s single-step walk could free-run past the target (+74 in the
entropy_draw goal-poll repro). The root cause was correctly re-isolated this iteration:
the prior hypothesis ("an MMIO-write exit eats the trap", iteration-50) was already
handled by re-arming after `Ok(exit)`. The *new* and distinct mechanism is that an
**emulator-DELIVERED `Debug` exit** — KVM's singlestep hook firing when an emulated
MMIO instruction completes — consumes the `guest_debug` arming, so the next entry
free-runs. The fix re-asserts `set_singlestep(true)` on **every** `Debug` exit in the
stepping loop (idempotent; harmless for hardware-delivered `#DB`s).

The fix is one line of behavior change with a thorough comment. I independently
**verified it is load-bearing and the regressions catch the bug**: reverting just the
`set_singlestep` call in the `Debug` arm makes both new tests fail with loud
`Overshoot` (target 4096 → counted 4110; target 101 → counted 114), and restoring it
makes them pass. clippy (dh-vmm) is clean. The two new live tests pass on this box.

The fix is correct, minimal, well-justified, and properly fenced (the `Debug` arm only
runs inside the stepping branch; the far-approach path never single-steps, so it cannot
trigger the re-arm). Determinism is preserved: the change only affects host-side trap
*delivery* (when KVM stops the vCPU), never guest-visible state or retirement counts, so
no existing landing target moves.

## Verdict

**APPROVE**

No Critical or Important issues block the merge. There are two genuinely useful
follow-ups worth filing (neither blocks): (1) document/decide whether the sibling
`step_one_entry` helper — used by the injection chain in `runctl.rs` — needs the same
analysis, since it shares the Debug-exit-after-MMIO structure but stops at the first
`Debug` rather than free-running (it is *not* broken the same way, but the interaction
deserves an explicit note); (2) a minor doc-count error and a couple of small clarity
suggestions. Details in `01`/`02`.

## Verification performed (this review)

- Read `/tmp/iter83.diff`, the full `land_at` + `step_one_entry` + `set_singlestep`,
  the new tests, `mmio_stepper.asm`, and all four wiring sites
  (build.rs/lib.rs/elf_shape.rs).
- Built `nanokernel` (mmio_stepper.elf assembles and links at the load addr).
- Ran both new live regressions: **pass**.
- **Reverted the fix and re-ran:** both regressions **fail with loud Overshoot**
  (proves the regressions are real and the fix is load-bearing); restored the file
  (git diff clean).
- `cargo clippy -p dh-vmm`: clean.
- Searched all `set_singlestep` / `VcpuExit::Debug` / `step_one_entry` sites across the
  workspace (boundary.rs land_at + step_one_entry; runctl.rs injection chain;
  counting_semantics + landing_precision test harnesses).

## Stats

- Files reviewed: 5 changed + 3 cross-referenced (runctl.rs, counting_semantics.rs,
  landing_precision.rs).
- Critical: 0
- Important: 0
- Suggestions: 4
- Positive notes: 5
