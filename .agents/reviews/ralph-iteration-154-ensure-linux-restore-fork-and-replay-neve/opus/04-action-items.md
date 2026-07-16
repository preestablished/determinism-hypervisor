# Action Items

## Critical

- None

## Important

- [ ] `crates/dh-worker/src/service.rs:1482` Replace `resolve_create_vm` on `VerifyReplay`, `RestoreSnapshot`, and `Fork` with a base-image-only resolver so these non-boot paths no longer require or read kernel/initramfs blobs they do not use.

## Suggestions

- [ ] `crates/dh-worker/src/service.rs:156` Mark `boot_observer` as diagnostic/test-only public API, for example with `#[doc(hidden)]` and a short comment that the counters count loader attempts.

- [ ] `crates/dh-worker/tests/common/mod.rs:205` Make `ensure_cache_entry` tolerate concurrent acceptance tests by treating `AlreadyExists` as success when the destination hash matches and installing copied files via a temporary path plus rename.

- [ ] `crates/dh-worker/tests/replay_engine.rs:869` Add the exact `DH_M9_ALLOW_SKIP=0 cargo test ... --ignored --nocapture` command to the replay boot-once test's ignore message or adjacent comment.

- [ ] `crates/dh-worker/tests/restore_engine.rs:878` Add the exact `DH_M9_ALLOW_SKIP=0 cargo test ... --ignored --nocapture` command to the restore boot-once test's ignore message or adjacent comment.
