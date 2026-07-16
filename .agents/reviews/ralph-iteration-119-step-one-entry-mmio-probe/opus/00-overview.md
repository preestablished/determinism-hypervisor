# Overview

Reviewed branch `ralph/iteration-119-step-one-entry-mmio-probe` for bead `determinism-hypervisor-ife`.

Scope inspected:

- `crates/dh-vmm/src/boundary.rs`
- `crates/dh-vmm/src/inject.rs`
- `crates/dh-vmm/src/runctl.rs`
- `tests/nanokernel/asm/mmio_irq_stepper.asm`
- `tests/nanokernel/build.rs`
- `tests/nanokernel/src/lib.rs`
- `tests/nanokernel/tests/elf_shape.rs`

Summary:

- The new live probe covers the critical primitive sequence: land at an MMIO-adjacent boundary, queue vector `0x40`, enter via `step_one_entry`, service emulated-MMIO exits before the next debug boundary, then queue vector `0x41` at that returned boundary and enter again.
- The guest setup is valid for the intended probe shape: long-mode nanokernel, real IDT entries for vectors `0x40` and `0x41`, `sti`, the existing MMIO stepper cluster, and a guest-visible delivery table.
- The assertions are replay-stable: the test discovers the target dynamically, runs the same probe twice from a fresh boot, and compares the boundary tuple plus MMIO exit trace and delivered vector order.
- I do not see a production correctness issue in the changed code.

Validation run:

- `cargo test -p nanokernel`
- `cargo test -p dh-vmm step_one_entry_chained_injection_crosses_mmio_exactly_live -- --nocapture`
- `git diff --check`

All passed in this environment.
