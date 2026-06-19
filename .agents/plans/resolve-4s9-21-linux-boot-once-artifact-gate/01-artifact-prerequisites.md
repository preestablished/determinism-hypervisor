# Artifact Prerequisites

This bead is blocked on external M9 Linux artifacts. Do not close it from skip-mode evidence.

## Required Environment

Use a Linux x86_64 host with usable KVM and dirty-ring support. The tests call the worker service, boot a BzImage Linux fixture, stop at guest-sdk Ready EventKind 14, then snapshot/restore/fork/replay that state.

The following environment variables must be set:

```bash
DH_M9_BZIMAGE=/path/to/bzImage
DH_M9_INITRAMFS=/path/to/initramfs
DH_M9_BASE_IMAGE=/path/to/base-image
DH_M9_GAME_IMAGE=/path/to/game-image
DH_M9_IMAGE_CACHE=/path/to/image-cache-directory
DH_M9_ALLOW_SKIP=0
```

Path requirements enforced by `crates/dh-worker/tests/common/mod.rs`:

- `DH_M9_BZIMAGE` must be a readable regular file.
- `DH_M9_INITRAMFS` must be a readable regular file.
- `DH_M9_BASE_IMAGE` must be a readable regular file.
- `DH_M9_GAME_IMAGE` must be a readable regular file.
- `DH_M9_IMAGE_CACHE` must be a readable directory.

## Preflight Commands

Run these before the acceptance tests:

```bash
git checkout main
git pull --ff-only origin main
git status --short --branch
bd show determinism-hypervisor-4s9.21
printenv DH_M9_BZIMAGE DH_M9_INITRAMFS DH_M9_BASE_IMAGE DH_M9_GAME_IMAGE DH_M9_IMAGE_CACHE DH_M9_ALLOW_SKIP
test -f "$DH_M9_BZIMAGE"
test -f "$DH_M9_INITRAMFS"
test -f "$DH_M9_BASE_IMAGE"
test -f "$DH_M9_GAME_IMAGE"
test -d "$DH_M9_IMAGE_CACHE"
```

Expected `DH_M9_ALLOW_SKIP` value for final evidence:

```bash
test "$DH_M9_ALLOW_SKIP" = 0
```

Do not use `DH_M9_ALLOW_SKIP=1` for final closure. Skip mode only proves the test harness can skip cleanly when artifacts are absent.

## Code Sources for Prerequisites

- `crates/dh-worker/tests/common/mod.rs` owns artifact lookup, KVM checks, cache population, and `m9_linux_ready_snapshot`.
- `crates/dh-worker/tests/restore_engine.rs` owns the restore/fork boot-once acceptance test.
- `crates/dh-worker/tests/replay_engine.rs` owns the VerifyReplay boot-once acceptance test.
- `crates/dh-worker/src/service.rs::boot_observer` owns the process-local boot loader counters used by both tests.
