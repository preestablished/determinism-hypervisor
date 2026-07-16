# Determinism

The denied-MSR runtime treatment is deterministic in the narrow dispatch path: denied reads return fixed `0`, denied writes inject `#GP` except the explicit `IA32_BIOS_SIGN_ID` no-op, and the no-op is stable because reads also return `0`.

The new CPUID mask for `CPUID.(7,0).EDX[29]` correctly hides `IA32_ARCH_CAPABILITIES` advertisement. The remaining determinism issue is enforcement: the vCPU always receives the worker's freshly masked KVM CPUID table, but `machine_config_hash` uses the caller-provided `cpuid_table` and accepts empty/mismatched tables. That means state hashes and replay headers can claim a different CPU surface than the guest actually saw.

The live trace schema is useful: it separates Linux CPU compatibility MSRs, lAPIC-required MSR/MMIO, and unclassified buckets. It should become an acceptance gate, not just a reporting artifact.
