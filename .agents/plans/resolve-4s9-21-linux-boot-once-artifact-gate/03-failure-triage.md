# Failure Triage

Use this file only if one of the artifact-backed acceptance tests fails.

## Classify the Failure

First separate infrastructure failures from product failures.

Infrastructure or artifact failures usually mention:

- missing `DH_M9_*` environment variables
- unreadable files or non-directory image cache
- unwritable image cache or cache population failure
- KVM unavailable
- dirty ring unavailable
- BzImage/initramfs/image hash lookup failure
- malformed BzImage, initramfs, base image, or game image
- wrong artifact set, for example a guest/initramfs that never emits guest-sdk Ready EventKind 14
- boot artifact/config mismatch, including unexpected `MachineConfig.base_image_hash` or pv-blk backing hash

Do not patch product code for those. Fix the host/artifact setup and rerun the same final command.

Product failures usually mention:

- BzImage loader count is greater than `1`
- restored/forked `machine_config_hash` mismatch
- restored/forked/replayed `state_hash` mismatch
- EVTC or BLKO snapshot section mismatch
- `VerifyReplay` emits divergence instead of `Done`
- `Run until Ready` does not stop on guest-sdk Ready EventKind 14

Those require code investigation.

Before treating a failure as a product bug, confirm the artifact set is the
intended M9 fixture set. At minimum, capture `sha256sum` or `b3sum` output for
all four artifact files, record the exact paths, and compare them with the
expected fixture manifest or release notes for the runner environment. A bad
artifact set can look like a Ready, state-hash, or pv-blk product failure.

## If BzImage Load Count Increases

The invariant is that only `CreateVm` may invoke the Linux boot loader.

Start with:

```bash
rg -n "record_bzimage_load|boot_slot|ResolvedBoot::BzImage|load_bzimage|BzImage boot" crates/dh-worker/src crates/dh-worker/tests
```

Expected shape:

- `crates/dh-worker/src/service.rs::boot_slot_with_loaders` records BzImage loads.
- Restore, fork, replay, and VerifyReplay must rebuild runtime buses from snapshot/config state without calling `boot_slot`.
- `CreateVm` remains the loader path.

Likely repair areas:

- `crates/dh-worker/src/restore_engine.rs`
- `crates/dh-worker/src/fork_engine.rs`
- `crates/dh-worker/src/replay_engine.rs`
- `crates/dh-worker/src/runtime.rs`
- `crates/dh-worker/src/service.rs`

Do not mask the counter assertion. Fix the extra loader call.

## If MachineConfig or State Hash Mismatches

Check whether restore/fork/replay is reconstructing deterministic devices from the snapshot and canonical machine config rather than from boot artifacts.

Useful searches:

```bash
rg -n "machine_config_hash|config_hash|restore_snapshot|take_snapshot|runtime_hash_device_sections" crates/dh-worker/src crates/dh-worker/tests
rg -n "EVTC|BLKO|DEVICE_ID_DETCHANNEL|DEVICE_ID_PV_BLK|Bisection|FileBase|base_image_hash" crates/dh-worker/src crates/dh-worker/tests crates/dh-devices/src
```

Important invariants:

- The Ready snapshot `machine_config_hash` must survive restore and fork.
- The Ready `state_hash` must survive restore and fork without rerunning initramfs or READY setup.
- EVTC is detchannel runtime state and must round-trip through DHSNAP.
- BLKO is pv-blk overlay/device state and must round-trip through DHSNAP.

## If VerifyReplay Diverges

Start from the replay test failure output. If it reports `VerifyReplay divergence`, inspect:

```bash
rg -n "verify_replay_done|verify_replay|replay_segment|reseal|EpochOk|Divergence" crates/dh-worker/src crates/dh-worker/tests
```

Expected shape:

- VerifyReplay restores the initial snapshot from the input log base.
- Replay runs to the same Ready boundary.
- Replay verifies epoch hashes and the final Ready state hash.
- Replay must not invoke `boot_slot` or BzImage loading.

Do not weaken divergence checks. The fix should make replay reproduce the recorded Linux READY state.

## Review Requirement for Code Fixes

If code changes are needed, follow Ralph:

- claim or keep `4s9.21` assigned
- create a `ralph/iteration-N-...` branch from clean `main`
- make a focused fix
- run the acceptance and workspace gates from `02-acceptance-runbook.md`
- use two independent review subagents
- apply review fixes if needed
- no-ff merge to `main`
- push `main`
- if host/artifacts cannot be fixed in-session, append notes to `4s9.21` with
  the exact failing preflight or artifact validation output and keep the bead
  blocked rather than closing it
