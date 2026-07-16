# Acceptance

- Deterministic lAPIC behavior: not accepted. The pure model is deterministic, but mutable state is transient and side-effectful ICR writes are silently accepted.
- Linux early-boot APIC MSR/MMIO service: not accepted for production. The worker `Run` detchannel wrapper does not dispatch LAPIC MMIO/MSR exits to `LocalApic`.
- Run-control/replay integration: partial. Replay has LAPIC arms, and synthetic `DeviceRail` tests pass, but production run control misses the service path and runtime does not preserve LAPIC state between runs.
- Timer rejection/no host time: accepted for the modeled timer registers. No host timer path was added.
- No KVM irqchip/PIT/kvmclock creation: accepted. I saw no branch changes adding those forbidden KVM devices or paravirt leaves.
- Test adequacy: not accepted. Tests cover unit/synthetic paths, but not the production worker path, multi-run persistence, snapshot/restore/fork persistence, or live Linux boot acceptance.

Required before approval:

- Wire LAPIC handling into `service_exit_with_detchannel` or remove the duplicate dispatch.
- Persist LAPIC state across worker runs.
- Define and implement LAPC snapshot/restore behavior, or fail closed on any LAPIC state that would need persistence.
- Reject or model ICR side effects.
- Add tests for production run dispatch and LAPIC persistence boundaries.
