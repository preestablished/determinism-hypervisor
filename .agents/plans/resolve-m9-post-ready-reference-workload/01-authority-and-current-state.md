# Authority And Current State

## Local Authority

Use these files as the local source of truth:

- `docs/decisions/m9-linux-ready-and-block-device.md`
- `docs/ops/test-partitioning.md`
- `tests/determinism/tests/linux_ready.rs`
- `tests/determinism/tests/common/mod.rs`
- `crates/dh-worker/tests/linux_worker_api.rs`
- `crates/dh-worker/tests/common/mod.rs`
- `crates/dh-vmm/src/kvm.rs`
- `crates/dh-vmm/src/config.rs`
- `crates/dh-worker/src/proto_map.rs`
- `crates/dh-vmm/src/runctl.rs`
- `crates/dh-vmm/src/boundary.rs`
- `crates/dh-vmm/src/inject.rs`

The decision file says M9 uses deterministic `PvBlk` at MMIO
`0xD000_4000`, and the Linux guest driver or shim presents that device as
`/dev/vdb`. It also says READY is only guest-sdk EventKind 14
`Ready{unit, region_count, manifest_generation}` on detchannel after channel
init, Hello, autostart/control `Start{}`, and expected region registration.
Serial markers are diagnostic only.

The test partitioning doc says `DH_M9_INITRAMFS` must be the reference-workload
guest image, not the M2 smoke/autostart image, and that its baked
`/etc/detguest/boot.toml` must declare:

- `boot_toml_version = 1`
- `[autostart] unit = <reference-workload unit id>`
- the autostart unit's `[unit.control]`
- `protocol = "refwork-ctl"`
- `proto_version = 1`
- `game_dev = "/dev/vdb"`
- `[[expected_region]]` entries for `wram`, `framebuffer`, and `meta`, each
  with `layout_version = 1`

## Current Evidence

`4s9.23` is closed: Linux boot-to-READY determinism works.

The blocker is after READY:

- `4s9.26` attempted `linux_landing_counting` and found:
  - target `641381314` overshot to `641604579`;
  - target `641405076` overshot to `641405371`;
  - target `641429828` free-ran to terminal HLT at `641612218`.
- `4s9.28` review rejected a tentative implementation because the current
  fixture halts after READY before producing any pv-pad `FRAME_MARK`, and the
  pv-blk IO check was standalone device-level IO instead of guest-driven IO in
  the same recorded worker segment.
- `4s9.30` fails preflight with the current staged initramfs because
  `boot.toml` is the smoke manifest:
  - autostart unit 0 executes `/opt/autostart-trivial`;
  - unit 1 executes `/opt/print-lines`;
  - no `[unit.control]`;
  - no `refwork-ctl`;
  - no `/dev/vdb` control contract;
  - no expected-region list.

## Current Artifact Paths Observed

The lab box has these staged paths in previous command evidence:

```bash
DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage
DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img
DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache
```

Do not assume those paths contain the correct fixture. Inspect
`etc/detguest/boot.toml` from the initramfs before running acceptance gates.

## Important Code Seams

`tests/determinism/tests/linux_ready.rs` has a working direct VMM boot-to-READY
test. It is the right starting point for direct VMM Linux gates.

`crates/dh-worker/tests/linux_worker_api.rs` already has a strict
`assert_initramfs_boot_contract` preflight. Do not remove that preflight. It is
correctly rejecting the smoke fixture.

`dh_vmm::runctl::run_segment` handles `Until::NextSdkEvent`, `IcountBudget`,
`FrameBudget`, `TimerArm`, scheduled injections, epoch hashes, and final state
hashes. The implementation agent should reuse this path rather than adding a
Linux-specific run loop.

`dh_vmm::boundary::land_at` and `dh_vmm::inject::inject_at_boundary` are the
canonical exact-landing and interrupt-delivery paths. If a new fixture still
free-runs or overshoots from READY, fix the fixture or the READY stop boundary;
do not compare host exits as a substitute.

`crates/dh-vmm/src/kvm.rs`, `crates/dh-vmm/src/config.rs`, and
`crates/dh-worker/src/proto_map.rs` are the authority seams for forbidden
host-time and host-IRQ surfaces. Linux timer/IRQ evidence must prove the guest
does not get KVM PIT, IOAPIC, in-kernel irqchip, kvmclock, TSC-deadline, or
other raw host timer sources through these seams.
