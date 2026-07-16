# Tests

The new tests are targeted and pass here.

Executed:

```text
cargo test -p nanokernel
cargo test -p dh-vmm step_one_entry_chained_injection_crosses_mmio_exactly_live -- --nocapture
git diff --check
```

Results:

- `cargo test -p nanokernel`: passed, including `mmio_irq_stepper_table_gpa_matches` and the static ELF shape test.
- `cargo test -p dh-vmm step_one_entry_chained_injection_crosses_mmio_exactly_live -- --nocapture`: passed.
- `git diff --check`: passed.

Coverage notes:

- The live test is hardware-gated through `kvm_usable`, consistent with existing boundary and injection probes.
- `tests/nanokernel/tests/elf_shape.rs:222` pins `TABLE_GPA` against the Rust constant, and `tests/nanokernel/tests/elf_shape.rs:72` ensures the new guest is included in the static ELF shape sweep.
- There is no host-runnable drift pin that the new guest's `MMIO_BASE`, `ITERS`, or instruction cluster still match `mmio_stepper`. The live probe would catch a broken MMIO shape on KVM hosts, but non-KVM CI would only catch the ELF shape and `TABLE_GPA`.

Suggested non-blocking follow-up:

- Consider adding an `elf_shape` parser check that the `%define MMIO_BASE` and `%define ITERS` in `mmio_irq_stepper.asm` match `mmio_stepper.asm`, and optionally that the loop still contains two MMIO writes plus one MMIO read before the NOP pad. This would make accidental drift visible on hosts without KVM.
