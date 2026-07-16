# Tests

## Commands Run

- `bd prime` and `bd dolt pull`: completed.
- `git diff --check main...HEAD`: passed.
- `cargo test -p dh-vmm linux_lapic --lib`: passed, 6 tests.
- `cargo test -p determinism-tests --test linux_boot_trace trace_tests`: passed, 3 tests.
- `cargo test -p dh-worker lapc --tests`: completed but matched 0 tests, so it is not meaningful evidence.
- `cargo test -p dh-worker --test snapshot_engine full_snapshot_round_trips_through_the_real_store`: passed.
- `cargo test -p dh-worker --test restore_engine restore_preconditions_and_mismatches_fail_loudly`: passed.
- `rg -n "KVM_CREATE_IRQCHIP|KVM_CREATE_PIT|create_irq_chip|create_pit|kvmclock" crates`: found comments/tests and CPUID masking references, no enabled creation path.
- `cargo fmt --check`: failed. Branch-touched files are included in the formatting diff.

## Not Run

- I did not regenerate the ignored live Linux trace. There is an existing local `target/m9/linux_boot_trace.json` with `lapic_required=true`, `terminal_reason=icount_limit_reached(...)`, empty unclassified denied MSR/MMIO/IRQ-timer sets, and APIC MMIO/MSR evidence.
