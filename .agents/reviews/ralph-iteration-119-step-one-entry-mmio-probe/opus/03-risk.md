# Risk

Residual risk is low.

Primary risk reviewed: a queued interrupt followed by `step_one_entry` could cross emulated-MMIO exits and lose the single-step arming, causing a free-run or a non-replay-stable returned boundary. The new test directly drives that sequence and repeats it from fresh boots, so it is a good regression probe for the bead.

Remaining risks:

- The probe is direct rather than routed through `runctl::run_segment`. This is acceptable for the bead's `step_one_entry` exposure, but it does not exercise `runctl`'s `unwind_or!` sentinels or agenda grouping around same-icount injections.
- The guest's MMIO cluster shape is duplicated from `mmio_stepper` by source convention rather than pinned by a host-runnable drift test. If it drifts, KVM live testing should fail, but non-KVM test lanes may not catch it.
- `read_irq_stepper_table` trusts the guest-written count when sizing the vector read (`crates/dh-vmm/src/boundary.rs:514`). In this specific test the guest writes only two entries and the final equality constrains the value to length 2, so this is not a practical correctness risk.

No production code changes are required based on this review.
