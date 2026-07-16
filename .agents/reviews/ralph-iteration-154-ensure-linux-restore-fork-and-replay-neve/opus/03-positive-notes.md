# Positive Notes

- `crates/dh-worker/src/service.rs:3318-3322` keeps boot initialization isolated to `CreateVm`, while `crates/dh-worker/src/service.rs:3388-3403` now creates a fresh slot and restores from snapshot state without calling the loader.

- `crates/dh-worker/src/service.rs:156-183` provides simple atomic observer counters, and `crates/dh-worker/src/service.rs:2040-2052` instruments both ELF and bzImage loader dispatches at the single `boot_slot_with_loaders` chokepoint.

- `crates/dh-worker/src/restore_engine.rs:263-417` already restores the state needed to make the boot-call removal sound: machine config identity, TIME, lAPIC, entropy/device sections, vCPU state, counter reset, and dirty-set cleanup.

- `crates/dh-worker/tests/common/mod.rs:303-431` centralizes the M9 Linux fixture setup, image-cache population, KVM/CPUID gating, snapstore lifecycle, `CreateVm`, READY run, and READY snapshot capture. That keeps the new restore and replay tests focused on behavior rather than setup.

- `crates/dh-worker/tests/replay_engine.rs:868-937` checks that `VerifyReplay` preserves the Linux READY end hash and does not increment the bzImage loader count beyond the original `CreateVm`.

- `crates/dh-worker/tests/restore_engine.rs:877-1061` covers both `RestoreSnapshot` and `Fork` from a Linux READY snapshot, including restored/forked state hashes and EVTC/BLKO section equality.

- `tools/dh-cli/src/gate.rs:55-66` and `tools/dh-cli/src/run.rs:63-74` explicitly initialize `hash_device_sections: None`, which makes the intended "no device sections in these CLI segments" behavior unambiguous after the `Segment` initializer drift.
