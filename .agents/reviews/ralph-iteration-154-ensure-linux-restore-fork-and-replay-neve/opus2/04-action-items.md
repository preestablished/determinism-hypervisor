# Action Items

## Critical

- None

## Important

- [ ] `crates/dh-worker/src/service.rs:1482` Replace `resolve_create_vm(&config)` in `VerifyReplay` with a no-boot resolver that validates the recovered config and opens only `base_image_hash`, so replay does not require cached kernel/initramfs blobs after the initial snapshot exists.

- [ ] `crates/dh-worker/src/service.rs:3380` Replace `resolve_create_vm(&config)` in `RestoreSnapshot` with the same no-boot base-image resolver, and add a regression test proving restore still works after only boot blobs are removed from the image cache.

- [ ] `crates/dh-worker/src/service.rs:3496` Replace `resolve_create_vm(&parent_runtime.machine_config)` in fork child creation with the no-boot base-image resolver, so fork does not validate or load unused boot artifacts.

- [ ] `crates/dh-worker/tests/common/mod.rs:205` Make `ensure_cache_entry` concurrency-safe by handling `AlreadyExists` after `hard_link`, copying through a temporary file, verifying the temp hash, and publishing without clobbering an existing hash-keyed cache entry.

## Suggestions

- [ ] `crates/dh-worker/src/service.rs:156` Change the boot observer test API from global `reset()` plus absolute counters to a hidden snapshot/delta API so concurrent tests cannot erase each other's baselines.

- [ ] `crates/dh-worker/tests/replay_engine.rs:922` Clarify that the EVTC/BLKO comparisons are snapshot immutability checks, or remove them if the replay end-state hash is the intended coverage.

- [ ] `crates/dh-worker/tests/replay_engine.rs:869` Include the exact non-skipping M9 acceptance command in the replay test ignore message.

- [ ] `crates/dh-worker/tests/restore_engine.rs:878` Include the exact non-skipping M9 acceptance command in the restore test ignore message.
