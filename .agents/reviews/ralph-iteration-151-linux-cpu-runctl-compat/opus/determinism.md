# Determinism

The production changes preserve the deterministic model.

CPUID/replay hash:

- `ARCH_CAPABILITIES` is now masked from leaf 7 EDX, alongside existing entropy, timing, topology, KVM paravirt, and TSC-related masking.
- The CPUID table remains part of the `MachineConfig` hash preimage through the existing sorted leaf representation.
- The added test toggles the new leaf 7 EDX bit and verifies both config hash and state hash chain movement.

MSR behavior:

- Denied RDMSR remains fixed-value emulation: every denied read supplies `0`, avoiding host-derived MSR data in guest execution.
- Denied WRMSR still faults by default. The new `IA32_BIOS_SIGN_ID` exception is an explicit deterministic no-op, and reads of the same MSR remain fixed at `0`.
- lAPIC/x2APIC MSRs remain denied and classified as lAPIC-required, not silently promoted into Linux CPU compatibility.
- Raw TSC remains denied and unclassified for trace purposes.

Run-control/timer behavior:

- Timer conversion stays in instruction-counter space using deterministic ceil conversion and `start_icount + 1` clamping for expired timers.
- Interrupt deferral remains guest-state-driven and bounded; the new live test covers repeatability when IF never opens.

Trace/hash implications:

- The Linux boot trace schema moves to version 2 and gains fields for Linux CPU compat MSRs, unclassified denied MSRs, unclassified MMIO, and IRQ/timer exits. These are characterization outputs, not machine identity inputs.
- I did not see new host wall-clock, scheduler, CPUID host-placement, raw TSC, or in-kernel irqchip dependency introduced by this branch.
