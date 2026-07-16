# Action Items

No blocking changes needed.

Optional improvements:

- Add host-runnable drift pins tying `mmio_irq_stepper.asm`'s `MMIO_BASE` and `ITERS` to `mmio_stepper.asm`, plus a simple loop-shape check for the two writes and one read. This would improve non-KVM coverage.
- Add a future `runctl` integration test only if the goal expands from the `step_one_entry` primitive exposure to full agenda/unwind behavior for same-boundary injections.

Review status: approved as-is for bead `determinism-hypervisor-ife`.
