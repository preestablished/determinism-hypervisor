# Branch Review Overview

- Branch: `ralph/iteration-154-ensure-linux-restore-fork-and-replay-neve`
- Date: 2026-06-19
- Reviewer: Claude Opus
- Overall verdict: REQUEST_CHANGES

This branch removes the explicit `boot_slot` calls from `RestoreSnapshot` and `VerifyReplay`, adds a public boot-load observer so acceptance tests can assert that Linux bzImage boot initialization only happens during `CreateVm`, factors a shared M9 Linux READY-snapshot harness into `dh-worker` tests, adds ignored restore/fork/replay Linux boot-once acceptance tests, and updates two `dh-cli` `Segment` initializers for the new `hash_device_sections` field.

## Stats

- Files changed: 6
- Lines added/removed: +691 / -8
- Commits: 1 (`719d1af ralph: iteration 154 checkpoint - linux restore replay boot once`)

## Verification Performed

- `git diff main...HEAD --name-only`
- `git diff main...HEAD`
- `git log main..HEAD --oneline`
- Read all changed files requested in the review prompt.
- `git diff --check main...HEAD`
- `cargo test -p dh-worker --test restore_engine --test replay_engine -p dh-cli --tests --no-run`
