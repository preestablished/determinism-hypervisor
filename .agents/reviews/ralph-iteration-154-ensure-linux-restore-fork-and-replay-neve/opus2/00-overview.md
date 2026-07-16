# Overview

- Branch: `ralph/iteration-154-ensure-linux-restore-fork-and-replay-neve`
- Date: 2026-06-19
- Reviewer: Claude Opus (2nd reviewer)
- Overall verdict: REQUEST_CHANGES

This branch removes pre-restore boot initialization from Linux snapshot restore, replay, and fork paths, adds an x86 boot loader observer, adds ignored M9 Linux acceptance tests for restore/replay/fork boot-once behavior, factors common M9 artifact/cache helpers, and updates `dh-cli` Segment construction to set `hash_device_sections: None`. The KVM state restore model itself looks coherent because restore/replay rebuild the VM shell, validate the config hash, build the runtime bus, and then rely on snapshot sections for CPU, LAPIC, RAM, and device state. The remaining blocking concern is that the new no-boot runtime paths still resolve and validate unused boot blobs, which preserves a hidden dependency on kernel/initramfs cache entries after the loader call was removed.

## Stats

- Files changed: 6
- Lines added/removed: 691 added, 8 removed
- Commits: 1

## Verification Performed

- Read full branch diff with `git diff main...HEAD`
- Read changed file list with `git diff main...HEAD --name-only`
- Read each changed file in full
- Reviewed commit history with `git log main..HEAD --oneline`
- Ran `cargo test -p dh-cli --tests --no-run`
- Ran `cargo test -p dh-worker --test replay_engine --test restore_engine --no-run`
- Ran `cargo test -p dh-worker --lib --no-run`
