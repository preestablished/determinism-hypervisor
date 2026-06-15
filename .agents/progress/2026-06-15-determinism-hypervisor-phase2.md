# determinism-hypervisor phase 2 progress

Date: 2026-06-15

Scope: determinism-hypervisor only. The broader phase plan mentions
snapshot-store, reference-workload, guest-sdk, and control-plane, but this session
is constrained to the hypervisor repo and the hypervisor M4-M7 path.

Inputs reviewed:

- `/home/infra-admin/.agents/plans/preestablished-phase-2/dependency-order.md`
- `/home/infra-admin/.agents/projects/determinism-hypervisor/docs/phase2/gaps.md`
- `.agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md`
- `.agents/docs/phases/phase-2-fork-and-replay.md`
- Bead `determinism-hypervisor-ol1`

Plan sequence from the evidence:

1. Finish `determinism-hypervisor-ol1`, the slot manager and lease prerequisite
   for the worker daemon.
2. Use that as the base for `determinism-hypervisor-rfv`, the real `dh-workerd`
   gRPC service on TCP `:7400` and UDS with lifecycle RPCs wired to the engines.
3. Prove M6 through the real worker API, including `ListSlots`, `WatchSlots`,
   lease validation, capture neutrality, and no cross-slot interference.
4. Build the M7 1000-fork verified re-execution harness and artifact after M6.

Two subagent reviews were completed:

- Phase-alignment review: current `slot_manager.rs` work is on the M6 critical
  path, but does not itself close M6 because `dh-workerd` is still preflight-only.
- Implementation review: found three slot-manager issues. The zero-child Fork
  validation order, reclaim order dependence, and reclaimed-vs-faulted return
  contract were addressed in `crates/dh-worker/src/slot_manager.rs`.

Implementation progress:

- Fork validates slot id and lease before returning `ZeroChildFork`, preserving
  the stale-token contract expected by the worker API.
- Reclaim expiry now snapshots sweep-start state so a child in a lower slot id
  cannot thaw and free its parent in the same sweep.
- `reclaim_expired` now returns only slots actually released to `Empty`; expired
  Running slots are marked `Faulted` and released on the next sweep.
- Added tests for stale zero-child fork, missing-slot zero-child fork, and
  child-before-parent reclaim ordering.

Verification:

- `rustfmt --check crates/dh-worker/src/slot_manager.rs`
- `cargo test -p dh-worker slot_manager -- --nocapture`
- `cargo test -p dh-vmm slot_state_tests -- --nocapture`
- `cargo test -p dh-worker proto_map -- --nocapture`
- `cargo test -p dh-worker`

Note: workspace-wide `cargo fmt --check` currently reports formatting diffs in
`tests/nanokernel/tests/capture_manifest_interop.rs` and
`tests/nanokernel/tests/elf_shape.rs`. Those files are outside this
determinism-hypervisor slot-manager change and were not modified.
