# Tests

Commands run:

```text
cargo test -p nanokernel --test elf_shape mmio_irq_stepper -- --nocapture
cargo test -p nanokernel --test elf_shape every_guest_is_a_static_x86_64_exec_at_the_load_addr -- --nocapture
cargo test -p dh-vmm step_one_entry_chained_injection_crosses_mmio_exactly_live -- --nocapture
git diff --check
```

All passed.

The new nanokernel is host-runnable through the existing shape-test path: `mmio_irq_stepper_elf()` is included in `every_guest_is_a_static_x86_64_exec_at_the_load_addr` (`tests/nanokernel/tests/elf_shape.rs:56`), and the `TABLE_GPA` Rust/asm drift test is present (`elf_shape.rs:222`).

Test gap: the live VMM test currently validates broad behavior and determinism, but not the exact MMIO-crossing boundary. It should pin the discovered boundary by RIP/GPA sequence and use an exact post-step boundary assertion rather than `after_first.icount > discovered_after`.
