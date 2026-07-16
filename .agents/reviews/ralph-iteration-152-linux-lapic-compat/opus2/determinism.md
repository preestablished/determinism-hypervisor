# Determinism Review

The APIC exits are ordered correctly ahead of generic fallbacks:

- Record path handles lAPIC MMIO/MSR before bus/default-deny handling at `crates/dh-vmm/src/recording.rs:123`.
- Replay path mirrors that ordering at `crates/dh-worker/src/replay_engine.rs:107`.
- Trace handles lAPIC before `prepare_exit_for_trace` / `classify_exit` at `tests/determinism/tests/linux_boot_trace.rs:300` and `tests/determinism/tests/linux_boot_trace.rs:397`.

Timer and host-time posture looks deterministic:

- Unmasked LVT timer writes are rejected at `crates/dh-vmm/src/lapic.rs:250`.
- Nonzero timer initial count is rejected at `crates/dh-vmm/src/lapic.rs:261`.
- x2APIC MSRs are rejected at `crates/dh-vmm/src/lapic.rs:127` / `crates/dh-vmm/src/lapic.rs:138`.
- CPUID continues to mask x2APIC and TSC-deadline (`crates/dh-vmm/src/cpuid.rs:16` and `crates/dh-vmm/src/cpuid.rs:17`), and KVM paravirt/kvmclock leaves remain removed.

Snapshot/hash implications are not solved in this branch:

- `LocalApic` is outside `MmioBus`, so `dh_vmm::hash::device_sections()` cannot see it (`crates/dh-vmm/src/hash.rs:353`).
- Run-control and replay epoch/final links still pass `&[]` for device sections in core paths (`crates/dh-vmm/src/runctl.rs:718`, `crates/dh-vmm/src/runctl.rs:843`, `crates/dh-worker/src/replay_engine.rs:903`, `crates/dh-worker/src/replay_engine.rs:1170`).
- DHSNAP still writes empty `LAPC` v1 and restore rejects non-empty `LAPC`.

Deferral to `determinism-hypervisor-4s9.17` is acceptable for this iteration because that issue explicitly covers LAPC section versioning, state hash, snapshot, restore, fork, replay, VerifyReplay, and golden fixtures, and it blocks later Linux worker/timer gates. It is not acceptable to use this branch as evidence for persisted non-empty lAPIC state.

Primary determinism risk remains ICR: the model currently accepts ICR writes without deterministic delivery or loud rejection.
