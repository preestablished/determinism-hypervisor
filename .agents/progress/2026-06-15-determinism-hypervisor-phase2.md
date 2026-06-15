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

## Iteration 103: `determinism-hypervisor-rfv` start

Branch: `ralph/iteration-103-dh-workerd-grpc-service`

Beads:

- Checked `bd list` before starting, then `bd ready`.
- Claimed `determinism-hypervisor-rfv`, the P0 worker-daemon service bead.

Reviewed inputs:

- `/home/infra-admin/.agents/plans/preestablished-phase-2/dependency-order.md`
- `/home/infra-admin/.agents/projects/determinism-hypervisor/docs/phase2/gaps.md`
- Bead `determinism-hypervisor-rfv`

Plan after review:

1. Build the real `dh-workerd` service shell on the `ol1` slot-manager base.
2. Land transport and host-runnable worker/slot visibility first:
   `GetWorkerInfo`, `ListSlots`, status mapping, generated-client test.
3. Keep lifecycle/execution RPCs `UNIMPLEMENTED` until the service owns real
   KVM/store runtime state; do not fake M6 acceptance.
4. Wire mutating RPCs through a per-slot runtime table next, then add UDS M6
   acceptance and M7 harness evidence.

Two subagent reviews were completed for this plan:

- Phase/critical-path review agreed that branching from the slot-manager branch
  and working `rfv` is correct, but warned that M4/M5 evidence still precedes
  M6 sign-off and that `WatchSlots` belongs to later transition/event wiring.
- Code-feasibility review confirmed the generated trait requires all 17 methods
  and concrete stream associated types, and called out dependencies, no fixed
  `:7400` tests, no preflight in host-runnable tests, and preserving the
  existing enum-cast guard.

Implementation progress:

- Added `crates/dh-worker/src/service.rs` with `WorkerService`, `WorkerConfig`,
  tonic `HypervisorWorker` implementation, TCP+UDS serving helper, lease wire
  validation, and `SlotError -> tonic::Status` mapping with `ErrorDetail`
  details.
- Implemented real `GetWorkerInfo` and `ListSlots` over `SlotManager`.
- Implemented explicit `UNIMPLEMENTED` responses for mutating, introspection,
  replay, frame-capture, and `WatchSlots` RPCs until their real runtime/event
  ownership lands.
- Updated `dh-workerd` to preserve `--preflight` and add serving mode with
  defaults `0.0.0.0:7400` and `/run/dh/grpc.sock`, plus `--skip-preflight` and
  ephemeral-address-friendly CLI flags for development/testing.
- Added direct service tests and a generated tonic client test on
  `127.0.0.1:0`; tests do not bind production ports or require `/dev/kvm`.

Remaining blockers surfaced by review:

- The service still needs a real per-slot runtime table before lifecycle RPCs
  can succeed: `SlotVm`, bus, entropy, config, dirty ring/set, hash chain,
  counter/thread state, base snapshot, and pause/fault state. Filed as
  `determinism-hypervisor-8kb`, now a dependency of `rfv`.
- `CreateVm` needs a production image/kernel resolver; current boot paths use
  raw test ELF bytes. Filed as `determinism-hypervisor-p8g`, now a dependency
  of `rfv`.
- `RestoreSnapshotResponse.config` needs a decode/recover path for the MCFG
  section; the current restore engine receives a caller-provided config and
  compares canonical bytes. Filed as `determinism-hypervisor-797`, now a
  dependency of `rfv`.
- `ForkRequest.entropy_seeds` conflicts with the current fork engine contract
  that preserves the parent PRNG stream; this needs a design decision before
  wiring the public RPC. Filed as `determinism-hypervisor-3pk`, now a
  dependency of `rfv`.
- Blocking KVM/PMU engine work must not run on the Tokio reactor; later slices
  should use per-slot blocking workers or `spawn_blocking` with vCPU core
  pinning via `SlotManager::core_for`.

Verification for this slice:

- `cargo check -p dh-worker`
- `cargo fmt --check -p dh-worker`
- `cargo test -p dh-worker service -- --nocapture`
- `cargo test -p dh-worker proto_map -- --nocapture`

## Iteration 104: `determinism-hypervisor-8kb` runtime-table start

Branch: `ralph/iteration-104-dh-workerd-runtime-table`, stacked on
`ralph/iteration-103-dh-workerd-grpc-service`.

Beads:

- Checked `bd list` before starting.
- Claimed `determinism-hypervisor-8kb`, the P0 per-slot runtime table blocker
  for `determinism-hypervisor-rfv`.

Two subagent reviews were completed for the phase-2/gap plan:

- Dependency-order review confirmed `8kb` is the correct next concrete slice
  before any honest `rfv` lifecycle success, with `p8g`, `797`, and `3pk`
  still blocking full lifecycle semantics.
- Implementation/verification review agreed M7 must not start yet and called
  out the same runtime ownership shape: `SlotVm`, bus, entropy, config, dirty
  tracking, hash chain, counters/thread state, base snapshot, pause/fault state,
  and off-reactor KVM work.

Implementation progress:

- Added `crates/dh-worker/src/runtime.rs` with a fixed-size `RuntimeTable<T>`
  keyed by slot id and a concrete `SlotRuntime` owner for the real x86 runtime
  resources: `SlotVm`, `MmioBus`, `DetEntropy`, `MachineConfig`, dirty ring/set,
  `StateHashChain`, optional counter, base snapshot, boundary position,
  pause flag, and thread state.
- `WorkerService` now owns a runtime table alongside `SlotManager` on x86.
- Added a `spawn_blocking` lifecycle helper and routed `DestroyVm` through the
  runtime table before releasing the slot-manager lease. A missing runtime now
  returns `FAILED_PRECONDITION` and leaves the slot allocated instead of
  silently freeing bookkeeping state.
- Tightened `DetDevice` and `BlockBase` to `Send` so daemon-owned runtime buses
  can live behind the tonic service safely; updated the pv-blk test base from
  `Rc` to `Arc`.

Verification for this slice:

- `cargo check -p dh-worker`
- `cargo fmt --check -p dh-worker`
- `cargo fmt --check -p dh-devices`
- `cargo test -p dh-worker runtime -- --nocapture`
- `cargo test -p dh-worker service -- --nocapture`
- `cargo test -p dh-devices`
- `cargo test -p dh-vmm blk_fixture -- --nocapture`
- `cargo test -p dh-worker`

Remaining:

- `8kb` still needs the next lifecycle wiring slice to populate the table from
  real `CreateVm`/`RestoreSnapshot`/`Fork` construction paths.
- `rfv` remains blocked by `p8g` image/kernel resolution, `797` MCFG decode, and
  `3pk` fork entropy semantics before full API success can be claimed.

## Iteration 105: `determinism-hypervisor-797` MCFG decode

Branch: `ralph/iteration-105-dhsnap-mcfg-decode`, stacked on
`ralph/iteration-104-dh-workerd-runtime-table`.

Beads:

- Checked `bd list` before starting this resumed session.
- Claimed `determinism-hypervisor-797`, the RestoreSnapshotResponse MCFG
  recovery blocker for `determinism-hypervisor-rfv`.

Two subagent reviews were completed for the current plan:

- Dependency-order review confirmed that `8kb` remains the correct runtime-table
  track, but the new `iteration-105` branch should stay scoped to `797`.
- Implementation review recommended `RestoreSnapshot` runtime-table population
  as the next larger service slice, with `797` MCFG decode as a mandatory
  prerequisite and `p8g`/`3pk` left explicit for CreateVm/Fork.

Implementation progress:

- Added `MachineConfig::canonical_decode` in `dh-vmm`, the exact inverse of the
  frozen v1 canonical MCFG encoding. It validates the reconstructed config,
  rejects trailing/non-canonical bytes, supports ELF and BzImage boot variants,
  and recovers landing-only knobs to defaults because they are intentionally
  excluded from the preimage.
- Added `dh_worker::restore_engine::recover_machine_config`, which fetches the
  snapshot manifest from the real snapshot-store, validates the DHSNAP device
  blob, parses the `MCFG` section, checks `sec_version == 1`, and returns the
  decoded domain `MachineConfig`.
- Tightened `apply_dhsnap` so restore now decodes and validates the `MCFG`
  section before comparing it with the caller-built slot config.
- Added a snapshot-store-backed recovery test using a crafted DHSNAP `MCFG`
  container, plus pure `dh-vmm` decoder round-trip and rejection tests.

Verification:

- `cargo test -p dh-vmm canonical_decode -- --nocapture`
- `cargo test -p dh-worker recovers_machine_config_from_snapshot_mcfg -- --nocapture`
- `cargo test -p dh-worker --test restore_engine -- --nocapture`
- `cargo fmt --check -p dh-vmm -p dh-worker`
- `cargo check -p dh-worker`
- `cargo test -p dh-vmm config -- --nocapture`

Remaining surfaced by this slice:

- The domain `MachineConfig` canonical MCFG includes `cpuid_table` and
  `device_set`, but the current public proto `MachineConfig` does not expose
  those fields. Do not add a lossy mapper silently when wiring
  `RestoreSnapshotResponse.config`; track and resolve the wire-shape decision
  before claiming full `rfv` lifecycle success. Filed as
  `determinism-hypervisor-ysm`, now an `rfv` dependency.
