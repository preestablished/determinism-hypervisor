# Implementation Sequence

## Phase 1: Claim And Baseline

1. Run `bd show determinism-hypervisor-4s9.29`.
2. Move the bead out of `BLOCKED` only if the team convention allows it now that the dependencies are closed; otherwise comment that implementation is starting because the dependency blockers are resolved.
3. Confirm the worktree before editing:

   ```bash
   git status --short --branch
   ```

4. Confirm KVM access and the reference cores:

   ```bash
   test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm
   cat /proc/self/status | rg '^Cpus_allowed_list:'
   cat /sys/devices/system/cpu/online
   ```

5. Confirm the current guard behavior by executing the ignored guard once. This command is expected to fail before implementation:

   ```bash
   DH_M7_ACCEPT_GUEST=linux \
   cargo test -p dh-worker --test m7_fork_verify --release \
     linux_m7_acceptance_requires_real_linux_fixture \
     -- --ignored --nocapture
   ```

6. Confirm the M9 Linux corpus still passes before wiring M7 onto it:

   ```bash
   DH_M9_ALLOW_SKIP=0 \
   cargo test -p dh-worker --test m5_record_replay --release \
     linux_m5_record_replay_post_ready_corpus_reverifies \
     -- --ignored --nocapture
   ```

## Phase 2: Add Explicit-Core M9 READY Helper

File: `crates/dh-worker/tests/common/mod.rs`.

Add a helper that accepts explicit slot cores:

```rust
pub fn m9_linux_ready_snapshot_with_slot_cores_and_config<F>(
    test_name: &str,
    slot_cores: Vec<u32>,
    configure: F,
) -> TestResult<Option<M9LinuxReady>>
where
    F: FnOnce(&mut dh_vmm::config::MachineConfig),
```

Implementation notes:

- Reuse the body of `m9_linux_ready_snapshot_with_config`.
- Change only the worker-config construction so it uses the provided `slot_cores`.
- Keep the existing `m9_linux_ready_snapshot(test_name, slots)` and `m9_linux_ready_snapshot_with_config(test_name, slots, configure)` APIs by delegating to the new helper with `(0..slots)` cores.
- Validate `slot_cores` is not empty and preserve the caller's core order.
- Do not change M5 behavior except through the delegation.

This is necessary because M7 acceptance is explicitly core-pinned by `DH_M7_ACCEPT_SLOT_CORES=2-5`; using the current M9 helper directly would silently run on cores `0-3`.

## Phase 3: Introduce Guest-Mode Dispatch In M7

File: `crates/dh-worker/tests/m7_fork_verify.rs`.

Add a small local mode enum:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcceptanceGuest {
    Nanokernel,
    Linux,
}
```

Parse `DH_M7_ACCEPT_GUEST` as:

- unset or `nanokernel` -> `AcceptanceGuest::Nanokernel`
- `linux` -> `AcceptanceGuest::Linux`
- anything else -> panic with the allowed values

Remove `reject_unimplemented_linux_m7_acceptance` from the real acceptance tests once Linux mode exists. Delete the ignored guard test or replace it with a real Linux-specific smoke only if the smoke provides evidence and does not dilute the full acceptance command.

Keep the default guest mode as nanokernel so existing local and nightly commands keep their current behavior unless they opt into Linux.

## Phase 4: Refactor Harness Setup

Add an enum-backed setup type in `m7_fork_verify.rs`. Do not flatten Linux-owned fields out of `common::M9LinuxReady`; it owns the service, blocking snapstore client, tempdir, store runtime, and root lease lifetime.

```rust
enum AcceptanceHarness {
    Nanokernel {
        svc: WorkerService,
        store: snapstore_client::blocking::SnapstoreClient,
        root_lease: proto::Lease,
        root_snapshot: proto::SnapshotRef,
        machine_config_hash: [u8; 32],
        root_cumulative_icount: u64,
        root_cumulative_vns: u64,
        root_frame_counter: u32,
        _store_rt: tokio::runtime::Runtime,
        _store_handle: snapstore_server::build_server::ServerHandle,
        _store_dir: tempfile::TempDir,
        _image_cache: tempfile::TempDir,
    },
    Linux {
        ready: common::M9LinuxReady,
        root_cumulative_icount: u64,
        root_cumulative_vns: u64,
        root_frame_counter: u32,
    },
}
```

The exact field names can vary, but the ownership model should not. Add accessor methods for `guest()`, `svc()`, `store()`, `root_lease()`, `root_snapshot()`, `machine_config_hash()`, `root_cumulative_icount()`, `root_cumulative_vns()`, and `root_frame_counter()`.

For `Nanokernel`:

- preserve the existing image-cache tempdir, snapstore tempdir, `WorkerService::new`, `pad_echo_config`, and `create_root` flow;
- compute or carry `machine_config_hash` for shared DHILOG validation if convenient;
- set root cumulative counters and root frame counter from the root `TakeSnapshotResponse`; they are normally zero for `pad_echo`;
- preserve existing behavior and assertions.

For `Linux`:

- call `acceptance_slot_cores_or_skip()` first, as the nanokernel path does;
- pass the exact returned `slot_cores` to the new explicit-core M9 helper;
- configure `config.epoch_len = M9_LINUX_CHILD_EPOCH_LEN`;
- use `ready.lease` as the reusable root parent lease;
- use `ready.ready_snapshot_ref` as the root snapshot for lineage and `VerifyReplay`;
- use `ready.store` and `ready.svc` for log fetches and RPCs;
- set `root_cumulative_icount = ready.ready_snapshot.icount`;
- set `root_cumulative_vns = ready.ready_snapshot.vns`;
- set `root_frame_counter = ready.ready_snapshot.frame_counter`;
- retain the `M9LinuxReady` object so its store runtime, tempdir, service, and lease remain valid.

Recommended Linux constants, copied from the proven M5 corpus:

```rust
const M9_LINUX_CHILD_FRAMES: u32 = 5;
const M9_LINUX_CHILD_HARD_CAP: u64 = 5_000_000;
const M9_LINUX_CHILD_EPOCH_LEN: u64 = 745_000;
```

## Phase 5: Split Child Execution By Guest

Extend `ChildRecord` so shared validation does not depend on `RUN_BUDGET`:

```rust
struct ChildRecord {
    index: usize,
    slot_id: u64,
    snapshot: proto::SnapshotRef,
    state_hash: [u8; 32],
    input_log_id: Vec<u8>,
    segment_end_icount: u64,
    segment_end_vns: u64,
    cumulative_icount: u64,
    cumulative_vns: u64,
    frames_elapsed: u64,
    frame_counter: u32,
    meta_pvblk_checksum: Option<u64>,
}
```

For nanokernel children:

- keep the current `InjectInputs` path and require `scheduled == BURST_EVENTS`;
- run `IcountBudget(RUN_BUDGET)`;
- keep the `run.icount == RUN_BUDGET` and `run.vns == VNS_PER_SECOND` checks;
- store `segment_end_icount = RUN_BUDGET`, `segment_end_vns = VNS_PER_SECOND`, `cumulative_icount = run.icount`, `cumulative_vns = run.vns`, and `frames_elapsed = run.frames_elapsed`.

For Linux children:

- do not inject `PAD_SET` events;
- run:

  ```rust
  proto::run_request::Until::FrameBudget(M9_LINUX_CHILD_FRAMES)
  ```

- set `hard_icount_cap = M9_LINUX_CHILD_HARD_CAP`;
- require `run.reason == BudgetReached`;
- require `run.frames_elapsed == M9_LINUX_CHILD_FRAMES`;
- compute `segment_end_icount = run.icount.checked_sub(root_cumulative_icount)`;
- compute `segment_end_vns = run.vns.checked_sub(root_cumulative_vns)`;
- require `segment_end_icount > 0`;
- require `segment_end_icount <= M9_LINUX_CHILD_HARD_CAP`;
- read the `meta` region pv-blk proof before destroying the child if the M5 helper is straightforward to reuse;
- take a snapshot with `seal_input_log = Some(true)`;
- store the child snapshot `frame_counter`, `segment_end_icount`, `segment_end_vns`, `run.icount`, and `run.vns`.

Reuse `destroy_best_effort` on all error exits so child leases are not stranded.

## Phase 6: Split DHILOG Validation By Guest

Replace `validate_single_edge_lineage(root, child, log)` with:

```rust
fn validate_single_edge_lineage(
    guest: AcceptanceGuest,
    root: &proto::SnapshotRef,
    child: &ChildRecord,
    machine_config_hash: [u8; 32],
    log: &[u8],
) -> ParsedChildLog
```

Shared checks:

- `Lineage::new(&[log])` succeeds and has length 1.
- `lineage.root_base()` equals the root snapshot hash.
- `lineage.end_identity()` equals the child snapshot hash, child state hash, and child segment end icount.
- `LogReader::parse(log)` succeeds.
- the DHILOG header base snapshot id equals the root snapshot hash.
- the DHILOG header end snapshot id equals the child snapshot hash.
- the DHILOG header end state hash equals the child state hash.
- the DHILOG header machine config hash equals the harness machine config hash.
- the DHILOG header end icount and end vns equal the child segment end counters, not the worker cumulative counters.

Nanokernel-specific checks:

- every canonical record is `PadSet`;
- canonical records exactly equal `expected_pad_records(child.index)`;
- parsed header end icount equals `RUN_BUDGET`;
- parsed header end vns equals `VNS_PER_SECOND`.

Linux-specific checks are defined in `03-linux-log-and-replay-contract.md`.

## Phase 7: Split VerifyReplay Assertions By Guest

Change `verify_child` so it receives `guest` and the parsed child log summary.

Shared checks:

- start `VerifyReplay` from the root snapshot and the child `input_log_id`;
- fail on any `Divergence`;
- fail on empty progress messages or stream errors;
- require exactly one `Done`;
- require `Done.end_state_hash == child.state_hash`;
- require `Done.total_icount == child.segment_end_icount`.

Nanokernel-specific checks:

- keep `EpochOk > 0`;
- keep `Done.total_icount == RUN_BUDGET`.

Linux-specific checks:

- require `EpochOk > 0`;
- require `EpochOk` count equals the parsed Linux `EPOCH_HASH` count;
- do not compare `Done.total_icount` to `RUN_BUDGET`.

## Phase 8: Preserve Full And Cross-Slot Orchestration

Keep the existing batching shape:

- one reusable root parent;
- `child_capacity = slot_cores.len() - 1`;
- batches of forks using `ForkRequest { parent, count, entropy_seeds }`;
- `child_seed(index)` for distinct-child batches;
- repeated same-seed forks for cross-slot checks.

For Linux full acceptance:

- assert `verified == jobs`;
- do not assert `unique_hashes.len() == jobs` unless the implemented Linux workload demonstrably consumes child entropy and makes this invariant stable;
- optionally log unique hash count for observability.

For Linux cross-slot acceptance:

- keep strict equality for same-seed children across slots:
  - snapshot ref;
  - state hash;
  - input log id;
  - DHILOG payload bytes;
  - parsed end icount and frame counter.

Same-seed cross-slot equality is the important determinism proof. Distinct-seed uniqueness is a nanokernel pad-script property, not part of the Linux acceptance wording.

## Phase 9: Nightly Workflow And Docs

File: `.github/workflows/nightly-drift.yaml`.

Add a Linux M7 canary rather than replacing the current nanokernel canary, unless CI capacity requires combining them later.

Recommended shape:

- keep the existing `m7-fork-verify-100` job as nanokernel/default coverage;
- add `m7-linux-fork-verify-100`;
- copy the existing M7 job setup rather than writing a shorter cargo-only job:
  - checkout this repository at `path: repo`;
  - checkout sibling path dependencies at `control-plane`, `guest-sdk`, and `snapshot-store`;
  - install the stable Rust toolchain;
  - preflight `/dev/kvm` read/write access;
  - preflight `nasm`;
- set:

  ```yaml
  DH_M7_ACCEPT_GUEST: linux
  DH_M7_ACCEPT_JOBS: ${{ inputs.m7_fork_jobs || '100' }}
  DH_M7_ACCEPT_SLOT_CORES: ${{ inputs.m7_slot_cores || '2-5' }}
  DH_M7_ACCEPT_ALLOW_SKIP: "0"
  DH_M9_ALLOW_SKIP: "0"
  DH_M9_BZIMAGE: /home/infra-admin/.cache/dh-m9/reference-workload/bzImage
  DH_M9_INITRAMFS: /home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio
  DH_M9_BASE_IMAGE: /home/infra-admin/.cache/dh-m9/reference-workload/base.img
  DH_M9_GAME_IMAGE: /home/infra-admin/.cache/dh-m9/reference-workload/game.img
  DH_M9_IMAGE_CACHE: /home/infra-admin/.cache/dh-m9/image-cache
  ```

- create or verify `DH_M9_IMAGE_CACHE` before running cargo;
- preflight `/dev/kvm`, `nasm`, and the five artifact paths before running cargo;
- run only the full 100-child Linux acceptance test in the nightly canary, not the cross-slot test, unless runtime measurements show both fit comfortably.
- add `m7-linux-fork-verify-100` to `alert-on-failure.needs`;
- update the alert title/body so a Linux M7 canary failure is visible in the failure issue.

File: `docs/ops/test-partitioning.md`.

Update M7 rows to include explicit Linux commands with `DH_M7_ACCEPT_GUEST=linux` and `DH_M9_ALLOW_SKIP=0`. Preserve the nanokernel/default rows so `4s9.31` remains straightforward.
