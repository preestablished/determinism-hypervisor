# Code Seams and Invariants

This file names the source surfaces another agent should inspect before changing code.

## Test Harness

`crates/dh-worker/tests/common/mod.rs`

- `M9LinuxArtifacts::from_env_required` validates the five artifact variables.
- `m9_masked_cpuid_table` requires KVM dirty-ring support unless skip mode is enabled.
- `populate_m9_image_cache` places artifact bytes in the image cache by hash.
- `m9_linux_ready_snapshot` creates the VM, snapshots the initial state, runs to Ready EventKind 14, and snapshots Ready state.
- `snapshot_section` reads EVTC/BLKO sections from stored DHSNAP containers.
- `verify_replay_done` consumes the VerifyReplay stream and requires terminal `Done`.

## Restore/Fork Acceptance

`crates/dh-worker/tests/restore_engine.rs::linux_boot_once`

Key assertions:

- `boot_observer::elf_loads() == 0`
- `boot_observer::bzimage_loads() == 1` after initial CreateVm
- BzImage load count remains `1` after `RestoreSnapshot`
- restored config hash equals `ready.config_hash`
- restored state hash equals `ready.ready_state_hash`
- BzImage load count remains `1` after `Fork`
- fork child state hash equals `ready.ready_state_hash`
- restored and forked EVTC sections equal Ready EVTC
- restored and forked BLKO sections equal Ready BLKO

## Replay Acceptance

`crates/dh-worker/tests/replay_engine.rs::linux_boot_once`

Key assertions:

- `boot_observer::elf_loads() == 0`
- `boot_observer::bzimage_loads() == 1` after initial CreateVm
- `VerifyReplay` returns `Done`
- BzImage load count remains `1` after VerifyReplay
- `Done.total_icount` equals Ready snapshot icount
- `Done.end_state_hash` equals Ready snapshot state hash
- Ready EVTC and BLKO sections are unchanged after replay

## Loader Counter

`crates/dh-worker/src/service.rs::boot_observer`

This process-local diagnostic counter is the explicit guard for the bead. Do not remove or weaken it.

`crates/dh-worker/src/service.rs::boot_slot_with_loaders`

This is where ELF and BzImage load counts are recorded. The only accepted BzImage count in these tests is the first CreateVm boot.

## Product Invariants

Allowed behavior:

- `CreateVm` resolves and boots the BzImage/initramfs once.
- Restore/fork/replay use snapshot, machine config, image cache, and deterministic device state.
- pv-blk source artifact bytes remain host inputs and are not mutated.
- detchannel EVTC and pv-blk BLKO snapshot sections round-trip exactly.

Disallowed behavior:

- Restore/fork/replay calling the BzImage loader.
- Restore/fork/replay redoing initramfs or guest READY setup.
- Treating serial output as READY evidence.
- Closing `4s9.21` from `DH_M9_ALLOW_SKIP=1` output.
