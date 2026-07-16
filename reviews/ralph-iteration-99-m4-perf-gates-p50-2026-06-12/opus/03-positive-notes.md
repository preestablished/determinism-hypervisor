# Positive Notes

### P1 — Every timed window is exactly the operation the gate names

This is the heart of a perf gate and it is right on all three:

- **Restore** (`tests/perf_gates.rs:198-211`, `benches/perf_gates.rs:170-191`): the fresh
  slot is created **outside** the timer (`sys.create_slot_vm(MEM)` before `Instant::now()`
  in the test, in the `iter_batched` setup closure in the bench). `restore_snapshot`'s
  signature takes an already-existing `&slot` (`restore_engine.rs:112`) — it never calls
  `create_slot_vm` — so this correctly honours §8.3's "RestoreSnapshot targets an existing
  slot." The comment at line 192-194 calls this out explicitly.
- **Fork** (`tests/perf_gates.rs:64-79`): `bus_c = test_bus()` and `drop(outcome)` (child
  teardown) both sit outside the timer; the only thing measured is `fork_slot`, which itself
  owns the child-slot creation (`fork_slot_vm`) and the codec apply — exactly the tier-A fork
  operation.
- **Snapshot** (`tests/perf_gates.rs:142-167`): the dirty-set rebuild is **before**
  `Instant::now()`, so the per-sample bitset re-population is not charged to the gate; only
  `take_snapshot` is timed.

### P2 — The dirty-set-cleared-on-ack invariant is correctly worked around

The instrument knows the engine clears the dirty set only after the store acks
(`snapshot_engine.rs:175-178`), so it rebuilds the 8k set at the top of every sample. Without
this, sample 2 onward would ship zero pages and the gate would measure an empty delta. The
comment ("the engine clears the set after the store acks — rebuild it") shows the author read
the engine, not just the signature.

### P3 — The `#[ignore]` + debug-refusal pattern is the correct response to the iteration-68/69 flake lesson

`#[ignore]` keeps perf assertions out of the parallel `cargo test` sweep where contention
makes them flaky; the `cfg!(debug_assertions)` guard refuses to gate a build where the engines
"measure the compiler, not the platform." The `#[ignore = "..."]` attribute embeds the exact
invocation, so an operator who hits the skip knows how to run it for real. This is disciplined.

### P4 — Honest, non-editorializing failure framing

The module docs and comments describe the thresholds as the IMPLEMENTATION-PLAN's figures and
do **not** soften them to make the run pass. The snapshot and restore gates are left as hard
`assert!`s that will fail loudly at 111.6 ms / 317 ms. The author measured, found two failures,
and escalated to 8ot rather than relaxing the gate — exactly the right move for an ACCEPT
instrument. The `pages_shipped == DIRTY_PAGES` and `pages_loaded == MEM/4096` assertions inside
the loops are a nice touch: they prove the load was actually 8k pages / a full 128 MiB image,
so a passing time can never be a "it shipped nothing fast" artifact.

### P5 — Bench gates cleanly without `/dev/kvm` and keeps the store small

`benches/perf_gates.rs:43-47` returns an empty benchmark set when KVM is unusable, so hosted
lanes that merely *compile* the bench never fail. The `criterion` dev-dep is x86_64-gated
(`Cargo.toml` target cfg), matching the crate-root `#![cfg(target_arch = "x86_64")]` on every
test target, so the dependency closure never pollutes non-x86 builds. Sample counts are
deliberately small with documented reasoning (32 MiB/sample).

### P6 — Custom criterion `main()` is configured correctly

`Criterion::default().configure_from_args()` picks up `--ignored`-style CLI args and
`c.final_summary()` emits the run summary — the two things the `criterion_main!` macro would
have done. Going custom is justified here because the KVM-availability early return needs to
run inside the harness before any benchmark group is built. Correct, minimal, and the
`#[path = "../tests/common/mod.rs"] mod common` reuse of the existing rig (rather than
duplicating `test_bus`/`spawn_store_blocking`) keeps the bench honest about using the same
fixtures the tests do.
