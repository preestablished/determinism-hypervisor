# Iteration 83 — Landing engine MMIO single-step fix (bead 4a3) — 2nd review

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-83-landing-mmio-fix`
- **Scope:** 5 files, +152/−1 — `crates/dh-vmm/src/boundary.rs` (the fix +
  2 live regressions), `tests/nanokernel/asm/mmio_stepper.asm` (new guest),
  `tests/nanokernel/{build.rs,src/lib.rs,tests/elf_shape.rs}` (registration).

## Summary

The fix re-asserts `set_singlestep(true)` on every `VcpuExit::Debug` arm of
`land_at`'s near-approach single-step loop. Root cause (well-isolated in the
commit message and the in-code comment): an emulator-delivered Debug exit —
the singlestep hook firing when an emulated MMIO instruction completes —
consumes the `guest_debug` arming, so the next entry free-runs to the next
natural exit (+18 in the probe, +74 in the iteration-82 goal-poll overshoot).
The diagnosis revises the original 4a3 hypothesis (which blamed MMIO-write
*completion* eating TF): TF survives completions; it is the emulator's Debug
delivery that disarms. The re-arm is an idempotent ioctl, applied
unconditionally because hardware #DBs and emulator Debugs are indistinguishable
at this layer. The mechanism, the cost statement (~1µs/step), and the failed
`immediate_exit` belt are all recorded.

The work is genuinely strong: correct root cause, a purpose-built long-mode
probe guest (the prior raw-code real-mode probes were vacuous and the author
caught that), two live regressions that exercise the exact iteration-82 shape
plus a 120-landing march, and three consecutive green suites including the
20k-landing M2 acceptance.

## The one finding that matters

`step_one_entry` (the chained-injection walk, runctl.rs:280) has its OWN
single-step loop with the SAME structural bug the fix just patched in
`land_at`: it re-arms TF only after a NON-Debug exit, never after a
`VcpuExit::Debug`. If an entry's single step lands on an emulated-MMIO
instruction whose completion delivers the emulator Debug, the NEXT entry under
that loop free-runs identically. It is NOT reachable by any guest committed
today (the chained-injection `i>0` path needs interrupt delivery, and the only
interrupt-delivering guests — timer_guest, sti_window — record into plain
guest RAM, not MMIO; pad_echo never enables IRQ_VECTOR). So this is a
LATER fix, not a now fix — but it is a latent landmine for the M5/M6
device-bus run loop (bead 40q's successor) where an injection boundary CAN sit
adjacent to a doorbell MMIO write. It should be a bead now, and ideally the
one-line re-arm applied now while the fix and its reasoning are fresh.

## Verdict

**APPROVE.** The fix is correct, minimal, well-justified, and well-tested. No
Critical or blocking issues in the shipped change. One Important follow-up
(the `step_one_entry` twin bug — file/fix), and a couple of small comment/
coverage-accuracy nits.

## Stats

| Class | Count |
|-------|-------|
| Critical | 0 |
| Important | 1 |
| Suggestions | 4 |
| Positive notes | 6 |
