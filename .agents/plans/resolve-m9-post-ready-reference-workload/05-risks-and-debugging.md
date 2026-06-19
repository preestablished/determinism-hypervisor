# Risks And Debugging

## False Positive Risk: Boot-To-READY Only

The largest risk is producing tests that pass by reusing boot-to-READY evidence
for post-READY requirements. Avoid this.

Every post-READY gate must prove that it executed after the Ready event:

- target icounts must be greater than `ready_icount`;
- DHILOG records must include post-READY records for the feature being tested;
- worker replay must verify the same segment that contains the feature records.

## False Positive Risk: Standalone Device Unit Test

A standalone `PvBlk` read/write unit test does not satisfy Linux M5/M7 worker
acceptance. The IO must be guest-driven and recorded in the same worker segment
whose DHILOG is replayed.

Use standalone device tests only for narrow regressions after the full worker
segment is covered.

## False Positive Risk: Zero-Test Filters

Some current worker commands accept Linux-looking env vars or filters that are
not wired. A `cargo test` command can report success after selecting zero tests.

Before accepting any Linux-filtered command:

- run the same selector with `-- --ignored --list`;
- verify nonzero Linux-specific tests are listed;
- inspect the test code to confirm it actually reads the Linux env var and
  boots the Linux fixture;
- include the nonzero test count in the bead note.

This is mandatory for M4/M5 worker tests, M5 corpus tests, and M7 Linux tests.

## Landing Failures After Fixture Replacement

If `land_at` still overshoots from the new READY stop:

1. Confirm the guest did not halt before the target.
2. Confirm the target is sufficiently far from READY and terminal exit.
3. Confirm the counter is reset and enabled before boot.
4. Confirm the READY event stop leaves the vCPU at a resumable instruction
   boundary.
5. Inspect whether the detchannel PIO exit needs a one-entry completion or a
   run-control park boundary before exact landing starts.

Do not fix this by comparing host-side KVM exit counts. The acceptance requires
guest retired-instruction counting.

## Timer/IRQ Failures

If scheduled injection fails:

- Check `dh_vmm::inject::injectable` conditions at the target.
- Confirm the Linux workload has IF enabled and no interrupt shadow at a stable
  post-READY point.
- Confirm no KVM in-kernel irqchip, PIT, IOAPIC, kvmclock, or TSC-deadline path
  is used.
- Confirm the vector is external interrupt range `>= 32`.

Do not add a host-time timer source to make the test pass.

## Replay Divergence

If `VerifyReplay` diverges after the fixture is replaced:

- Compare whether the divergence happens before READY or after READY.
- If before READY, revisit setup-data RNG seed, initramfs boot order, jiffies
  clocksource, CPUID masking, and detchannel guest-RAM snapshot coverage.
- If after READY, inspect the new workload for PID-like, timestamp-like, or
  host-random bytes in hashed regions.
- Use page-diff evidence in the bead note; do not hide the divergence behind an
  allowlist unless a decision document accepts it.

For `4s9.30`, this is an owned unblock phase, not just debugging. The bead is
not closable until the known Linux `VerifyReplay` divergence is resolved or a
superseding M9 scope decision removes VerifyReplay from acceptance.

## Artifact Drift

Artifact paths are mutable local staging paths. Always record BLAKE3 hashes in
evidence notes. If a test passes once and later fails, re-hash the artifacts
before changing code.

## Scope Control

This plan is deliberately about unblocking the fixture. It should not be used
to:

- implement deterministic virtio-blk without a superseding bead;
- weaken `[unit.control]` or expected-region preflights;
- mark docs/evidence beads complete before underlying gates pass;
- remove nanokernel tests or golden fixtures.
