# Findings

## REQUEST_CHANGES

1. P2: APIC ICR writes are silently accepted but neither delivered nor rejected.

   `crates/dh-vmm/src/lapic.rs:248` and `crates/dh-vmm/src/lapic.rs:249` store `REG_ICR_LOW` / `REG_ICR_HIGH` and return `Ok(())` for all values. There is no delivery implementation for self/external IPIs and no loud unsupported-path error comparable to the timer rejection at `crates/dh-vmm/src/lapic.rs:250`. Because lAPIC exits are intercepted before the generic denied paths in record, replay, and trace (`crates/dh-vmm/src/recording.rs:123`, `crates/dh-worker/src/replay_engine.rs:107`, `tests/determinism/tests/linux_boot_trace.rs:441`), an ICR write would disappear from denied/unclassified evidence while also not causing the requested interrupt.

   The existing local `target/m9/linux_boot_trace.json` does not show `0xfee00300` or `0xfee00310`, so this may not affect the currently observed Linux early-boot path. Still, the lAPIC model should either reject non-benign ICR delivery commands loudly or add acceptance evidence and regression coverage proving the supported Linux path never issues them before READY.

2. P3: Formatting gate fails in branch-touched files.

   `cargo fmt --check` fails. The output includes unrelated pre-existing formatting diffs, but it also includes branch-touched code at `crates/dh-vmm/src/recording.rs:123` and `tests/determinism/tests/linux_boot_trace.rs:17`. This is not a behavioral regression, but it is an actionable merge-quality issue.

## Non-Blocking Risk

The new `LocalApic` state is mutable (`crates/dh-vmm/src/lapic.rs:50`) and is held on `DeviceRail` (`crates/dh-vmm/src/recording.rs:75`), but snapshot/restore/hash still treat `LAPC` as empty v1 (`crates/dh-worker/src/snapshot_engine.rs:295`, `crates/dh-worker/src/restore_engine.rs:303`) and state-hash device sections are bus-only (`crates/dh-vmm/src/hash.rs:351`). Deferral to `determinism-hypervisor-4s9.17` is acceptable only if this branch is not presented as snapshot/replay/VerifyReplay-complete for non-empty lAPIC state.
