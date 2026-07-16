# Correctness

## Finding C1: MMIO boundary exactness is under-asserted

Severity: Medium

`discover_mmio_crossing_entry` returns the counter value before the first `step_one_entry` call that observes any MMIO exit, plus the resulting `after.icount` (`crates/dh-vmm/src/boundary.rs:525`). The main test then asserts only that the injected entry saw at least one write and one read (`boundary.rs:597`) and that `after_first.icount > discovered_after` (`boundary.rs:602`).

That proves the chain reached an MMIO-heavy entry, but it does not fully prove the comment-level claim that the vector was queued at the boundary immediately before the MMIO cluster and that `step_one_entry` returned at the exact next retired boundary. A regression that lets `step_one_entry` run extra retired instructions after the MMIO cluster could still satisfy `> discovered_after`; likewise, a discovery point that is merely "an entry that eventually saw MMIO" is not pinned by RIP or exact MMIO GPA sequence.

The delivery assertions have the same weakness: `inj1.delivered_icount == target` and `inj2.delivered_icount == after_first.icount` assert the queue-time boundary reported by `inject_at_boundary`, while the ISR table proves delivery order, not the exact post-delivery boundary.

Suggested fix: make discovery return the full before/after `Boundary` tuple and the exact MMIO exit sequence, then assert those values in the injected run. At minimum, assert the fresh landing RIP matches the discovered RIP, assert the first MMIO entry touches exactly `MMIO_BASE + 0x14`, `MMIO_BASE + 0x08`, and `MMIO_BASE + 0x18` in order, and replace the loose `after_first.icount > discovered_after` with an exact relation derived from a no-interrupt baseline plus the deterministic ISR retirement delta.

## Covered Areas

I do not see an ISR register-clobbering bug in the new guest. The loop keeps `MMIO_BASE` in `rbx` (`tests/nanokernel/asm/mmio_irq_stepper.asm:45`), and the `RECORD` macro saves/restores both registers it uses (`rax`/`rbx`) before `iretq` (`mmio_irq_stepper.asm:74`). The handler does not touch `rcx`, which is the loop counter.

The GDT/IDT setup follows the existing `timer_guest` pattern closely enough for this probe shape, including an in-memory GDT before interrupt delivery.
