# Suggestions

### Suggestion: Prefer boot observer snapshots over global reset in tests

- File: `crates/dh-worker/src/service.rs:156`

The observer counters are concurrency-safe atomics, but `boot_observer::reset()` is a global mutation. That is acceptable for the current ignored tests when they run alone, but it is easy to misuse in future tests or in a shared test process with other service instances. A snapshot/delta API would avoid resetting global state while still proving that restore/replay did not invoke a loader.

```rust
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootCounts {
    pub elf: u64,
    pub bzimage: u64,
}

#[doc(hidden)]
pub fn snapshot() -> BootCounts {
    BootCounts {
        elf: ELF_LOADS.load(Ordering::SeqCst),
        bzimage: BZIMAGE_LOADS.load(Ordering::SeqCst),
    }
}
```

Tests can then capture a baseline before creating the VM and assert count deltas rather than resetting process-wide counters.

### Suggestion: Clarify or remove replay snapshot section comparisons

- File: `crates/dh-worker/tests/replay_engine.rs:922`

The replay test captures `ready_evtc` and `ready_blko`, runs `VerifyReplay`, then reads the same `ready_snapshot_ref` again. That proves the stored snapshot entry was not mutated, but it does not independently validate replayed device sections. The meaningful replay assertion is `done.end_state_hash == ready.ready_state_hash`.

If the section comparison is intended as an immutability guard, add a short comment. Otherwise, remove it to keep the test focused.

```rust
// VerifyReplay must not mutate the Ready snapshot while using it as the target.
assert_eq!(
    common::snapshot_section(&ready.store, &ready.ready_snapshot_ref, tag::EVTC)
        .expect("Ready EVTC section after replay"),
    ready_evtc
);
```

### Suggestion: Put the exact M9 gate command in ignore messages

- File: `crates/dh-worker/tests/replay_engine.rs:869`, `crates/dh-worker/tests/restore_engine.rs:878`

The ignored tests allow local skipping through `DH_M9_ALLOW_SKIP`, which is useful for development. For acceptance evidence, the ignore text should spell out the non-skipping command so the expected gate invocation is harder to run in a silently skipped mode.

```rust
#[ignore = "M9 Linux gate: DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture"]
```

Use the matching `restore_engine` command on the restore test.
