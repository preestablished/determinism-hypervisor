# Positive Notes

- `tests/determinism/tests/linux_boot_trace.rs:136` uses `BTreeMap` and `BTreeSet` for trace collections, which keeps JSON field ordering deterministic across runs.
- `tests/determinism/tests/linux_boot_trace.rs:193` derives `lapic_required` from observed APIC MMIO/MSR accesses instead of hard-coding a conclusion.
- `tests/determinism/tests/linux_boot_trace.rs:323` reuses `dh_vmm::kvm::classify_exit`, so denied MSR reads/writes continue to go through the existing deterministic policy rather than a bespoke trace-only path.
- `tests/determinism/tests/linux_boot_trace.rs:476` includes `schema_version`, limits, final instruction count, terminal reason, and required M9 trace fields in one stable artifact.
- `tests/determinism/tests/linux_boot_trace.rs:628` adds a host-runnable serializer test, so the JSON shape has at least some coverage without requiring KVM or external Linux artifacts.
- `tests/determinism/tests/linux_boot_trace.rs:106` preserves the CPUID masking assertions for kvmclock, x2APIC, TSC-deadline, RDRAND, RDSEED, RDTSCP, and invariant TSC.
