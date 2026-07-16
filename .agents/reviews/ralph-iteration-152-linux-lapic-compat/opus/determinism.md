# Determinism

Positive observations:

- The new `LocalApic` is plain Rust state and does not create a KVM irqchip, PIT, kvmclock, host timer, or TSC-deadline source.
- x2APIC MSRs are rejected by the model, and the existing CPUID mask continues to hide x2APIC and TSC-deadline from Linux.
- LAPIC timer use is fail-closed for unmasked LVT timer writes and nonzero initial-count writes. Current count reads as zero, so no host time is introduced.
- Existing KVM construction still avoids `KVM_CREATE_IRQCHIP`, PIT, and KVM paravirt leaves.

Determinism risks:

- Production `Run` currently bypasses the LAPIC service, so Linux early-boot APIC exits remain unhandled in the worker path.
- LAPIC state is mutable but not owned by slot runtime, not included in LAPC snapshots, and not restored. This breaks deterministic continuation after run boundaries, snapshots, restores, forks, and bisection checkpoints.
- Unsupported ICR writes are acknowledged instead of rejected, which can silently drop guest-visible interrupt side effects.
- If LAPIC state becomes part of the machine model, it also needs explicit state-hash/preimage treatment rather than living only in a transient rail object.
