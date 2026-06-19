# Contract Sources

This plan follows the upstream project docs in `/home/infra-admin/.agents/projects/determinism/` plus this repository's local M9 decisions.

## Upstream Determinism Docs

Guest SDK:

- `/home/infra-admin/.agents/projects/determinism/docs/guest-sdk/ARCHITECTURE.md`
  - section 4.1: deterministic READY point.
  - section 4.2: agent control leg `Hello -> LoadGame{dev_path} -> Start{} -> Ready`.
  - READY guarantees no host input before the Ready doorbell and a root snapshot that is a pure function of the workload image.
- `/home/infra-admin/.agents/projects/determinism/docs/guest-sdk/API.md`
  - section 3.1: EventKind table, including `Ready` kind 14.
  - section 3.2: `Ready` payload is `unit u32`, `region_count u32`, `manifest_generation u64`.
  - section 7: boot manifest uses `[unit.control] game_dev = "/dev/vdb"` and expected regions such as `wram`, `framebuffer`, and `meta`.
  - drift note: upstream text still describes `game_dev` as a virtio-blk game-image device in places. Local M9 decision `docs/decisions/m9-linux-ready-and-block-device.md` overrides that transport to the existing deterministic pv-blk device.
- `/home/infra-admin/.agents/projects/determinism/docs/guest-sdk/INTEGRATION.md`
  - the agent reads `boot.toml`, uses `game_dev`, drives `Hello -> LoadGame -> Start`, and emits ring-A Ready only after expected regions are registered.

Determinism hypervisor:

- `/home/infra-admin/.agents/projects/determinism/docs/determinism-hypervisor/ARCHITECTURE.md`
  - section 2.3: Linux direct boot with forced deterministic cmdline baseline.
  - section 6.5: pv-blk at `0xD000_4000`, read-only base plus CoW overlay.
  - section 6.6: detchannel host side, CHANNEL_INIT, doorbell drain, and canonical DEV_EVENT logging.
  - section 8: snapshots carry deterministic device state.
- `/home/infra-admin/.agents/projects/determinism/docs/determinism-hypervisor/API.md`
  - section 2.4: `RunRequest.next_sdk_event` stops at the next matching detchannel EventKind.
  - section 4: DHSNAP carries `EVTC` and `BLKO` device sections.
  - section 5: snapshot refs and state hash depend on canonical bytes.
- `/home/infra-admin/.agents/projects/determinism/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md`
  - M9: minimal Linux guest path uses bzImage boot, deterministic cmdline, and boot-to-READY deterministic tests.

## Local Repository Decisions

- `docs/decisions/m9-linux-ready-and-block-device.md`
  - local authority for M9 device transport.
  - selects existing deterministic pv-blk at `0xD000_4000`.
  - Linux fixture maps that pv-blk device to `/dev/vdb`.
  - rejects deterministic virtio-blk work for M9 unless superseded by a new bead.
  - rejects serial-only readiness.
- `docs/decisions/m9-linux-cmdline-policy.md`
  - local authority for the canonical Linux cmdline.
  - `BzImageBoot.cmdline` is append-only extras, with only `quiet` and `loglevel=<n>` accepted.
  - config hashing uses the canonical bytes.
- `docs/upstream-divergences.md`
  - use only if this plan uncovers a real local divergence that must be recorded.

## Current Code Anchors

- `crates/dh-vmm/src/boot.rs`
  - `load_bzimage_and_enter` already loads a deterministic-subset Linux bzImage and initramfs and enters Linux 64-bit state.
- `crates/dh-worker/src/image_resolver.rs`
  - `resolve_create_vm` already resolves `BootSpec::BzImage` into `ResolvedBoot::BzImage { kernel, initramfs, cmdline }`.
- `crates/dh-worker/src/service.rs`
  - `build_bus` already registers pv-clock, pv-pad, pv-entropy, pv-blk, debug serial, pv-net, and detchannel according to `MachineConfig.device_set`.
  - `boot_slot` is the missing worker BzImage dispatch.
  - `until_from_run_request` is the missing `NextSdkEvent` dispatch.
  - `service_exit_with_detchannel` already drains detchannel events at doorbell exits.
- `crates/dh-vmm/src/runctl.rs`
  - `Until::NextSdkEvent` already exists and expects a caller-fed matching SDK event counter.
- `crates/dh-vmm/src/hash.rs`
  - `device_sections(&MmioBus)` already frames deterministic bus device state for the state-hash preimage.
  - `lapic_section(&LocalApic)` frames deterministic lAPIC state.
- `crates/dh-worker/src/snapshot_engine.rs`
  - DHSNAP already writes bus device sections, including `EVTC` and `BLKO`, in canonical order.
- `crates/dh-worker/tests/common/mod.rs`
  - M9 artifact env vars and `M9LinuxArtifacts::from_env_required` already exist.
