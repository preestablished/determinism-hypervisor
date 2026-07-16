# Acceptance

## Evidence Present

- Existing `target/m9/linux_boot_trace.json` reports `lapic_required=true`.
- The trace reports no unclassified denied MSRs, MMIO addresses, or IRQ/timer exits.
- The trace records APIC MMIO addresses including ID/version/TPR/SVR/ISR/IRR/ESR/LVT registers and APIC base MSR `0x1b`.
- The trace does not record ICR addresses `0xfee00300` / `0xfee00310`.
- Targeted lAPIC tests verify reset values, served reads/writes, interrupt queue helpers, timer rejection, x2APIC rejection, and APIC-before-bus ordering.
- Static search found no new enabled KVM irqchip/PIT/kvmclock creation path.

## Gaps

- I did not regenerate the ignored live trace in this review session.
- ICR delivery semantics are not covered by tests or by a loud unsupported-path check.
- `cargo fmt --check` fails.
- Persisted LAPC snapshot/hash/restore/replay coverage is deferred to `determinism-hypervisor-4s9.17`; this branch should not close that requirement.

## Scope Judgment

For `4s9.16`, the branch has credible evidence for early Linux APIC probe compatibility, timer rejection, no host-time timer source, and no KVM irqchip creation. Acceptance is incomplete until the ICR behavior is made explicit and formatting passes.
