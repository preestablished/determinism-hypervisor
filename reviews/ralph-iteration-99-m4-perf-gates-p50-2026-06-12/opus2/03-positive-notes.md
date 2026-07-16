# Positive Notes

## P1 — The fork loop's drop placement is correct and prevents child accumulation

`tests/perf_gates.rs:119` drops the `ForkOutcome` immediately after `t.elapsed()` is recorded, inside the loop body. `ForkOutcome` owns `child: SlotVm`, whose CoW memfd and per-slot KVM fds are released on drop — so each of the 30 children is torn down before the next sample, and the comment ("child teardown outside the timed window of the NEXT sample") is precise: teardown lands in neither this sample's nor the next sample's timed region. No 30-deep memfd pile-up, no descriptor exhaustion. The frozen parent's memfd is shared CoW and untouched (fork_engine.rs never modifies the parent), so the parent stays valid across all samples. Clean.

## P2 — Dirty-set rebuild correctly models the engine's clear-on-ack contract

`snapshot_engine.rs:176-178` clears the dirty set only after the store acks (§8.2's last step). The test mirrors this by rebuilding the 8192-entry bitset at the top of each sample (`tests/perf_gates.rs:155-158`). This is the right shape: it reproduces the per-snapshot cost the engine actually pays and keeps each sample independent rather than measuring a one-shot drain followed by 29 empty incrementals. The reasoning that the host `write_slice` fills don't enter the KVM dirty ring (so the SET, not guest execution, defines the 8k load) is correct — `harvest_at_boundary` will find an empty ring and the explicit `dirty.insert` calls are what drive the page list. The `let mut ring`/`let mut dirty` reuse across samples is consistent: the ring stays empty (nothing dirties guest RAM between iterations through KVM), and the set is explicitly rebuilt, so no stale state leaks between samples.

## P3 — Disciplined gate hygiene: release-only, debug self-skip, KVM self-skip, `#[ignore]`d, single-threaded

The test refuses to gate a debug build (`debug_assertions` early-return with a loud message), self-skips without `/dev/kvm`, and is `#[ignore]`d so it never runs in the parallel `cargo test` sweep (the iteration-68/69 flakiness lesson is cited and applied). The exact reproduction command is in both the module doc and the `#[ignore]` reason string. These are precisely the guards a perf-acceptance test needs to avoid producing meaningless numbers in the wrong environment, and they are all present and correct.

## P4 — Real store, real engine path (R12 honored end to end)

The instruments drive the actual `SnapstoreClient` blocking facade against a real in-process `snapstore-server` (via `spawn_store_blocking`), not a mock. `take_snapshot` returns only after the store durably acks (the ref is the durability receipt), and restore goes through `resolve_pages` + the full DHSNAP decode/apply path. The measured numbers reflect the production seam, including fsync — which is exactly why the snapshot/restore FAILs trace the box's ext4 dd-fsync floor. The measurement is honest about the platform; the failure is a real platform/threshold signal, correctly escalated to bead 8ot rather than papered over.

## P5 — Correct separation of the timed window from setup in both instruments

In all three surfaces, only the operation under test is inside `Instant::now()`/`elapsed()` (test) or the criterion closure (bench). Slot creation for restore is explicitly pulled outside the timed window with the §8.3 justification ("RestoreSnapshot targets an existing slot"), `test_bus()` construction for fork is built before the clock starts, and the dirty-set rebuild for snapshot is outside `Instant::now()`. The criterion bench uses `iter_batched` with `BatchSize::PerIteration` for fork and restore so per-iteration setup is not charged to the sample — the idiomatic and correct criterion pattern for this. The boundaries are drawn exactly where the gate semantics require.
