# Findings

No blocking correctness findings.

I reviewed the branch diff against `main` with focus on Linux early-boot CPU compatibility, MSR filtering/emulation, CPUID masking, run-control timer/injection behavior, replay determinism, and hash preimage effects. I did not find an actionable production bug requiring changes.

Relevant reviewed surfaces:

- `crates/dh-vmm/src/cpuid.rs:96`: masks CPUID leaf 7 subleaf 0 EDX `ARCH_CAPABILITIES`, avoiding exposure of host vulnerability/mitigation MSR surface.
- `crates/dh-vmm/src/msr.rs:135`: classifies denied Linux early-boot CPU-probe MSRs separately from lAPIC-required and unclassified surfaces.
- `crates/dh-vmm/src/msr.rs:158`: acknowledges `IA32_BIOS_SIGN_ID` WRMSR as an explicit deterministic no-op while preserving `#GP` for other denied writes.
- `crates/dh-vmm/src/kvm.rs:667`: applies denied RDMSR/WRMSR policy in the KVM exit dispatch before re-entry, which is the determinism-critical place for `kvm_run.msr.error` and supplied data.
- `tests/determinism/tests/linux_boot_trace.rs:368`: avoids zero-filling detchannel/serial `IN` exits and now records unclassified MMIO/MSR/IRQ-timer surfaces in the trace artifact.

Residual risk: full Linux artifact-backed boot characterization was not runnable in this environment because the required `DH_M9_*` artifact paths were unset. That is a coverage limitation, not a branch correctness finding from local review.
