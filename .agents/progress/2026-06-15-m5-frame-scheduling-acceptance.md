# M5 frame scheduling acceptance progress

Date: 2026-06-15

Branch: `ralph/iteration-106-m5-frame-scheduling-acceptance`

Bead: `determinism-hypervisor-5yo` - M5 ACCEPT: `at_frame`
scheduling + `frame_budget` stops vs FRAME_MARK table, including across
snapshot/restore.

Status: closed after implementation and verification.

## Inputs reviewed

- `/home/infra-admin/.agents/plans/preestablished-phase-2/dependency-order.md`
- `/home/infra-admin/.agents/projects/determinism-hypervisor/docs/phase2/gaps.md`
- `bd list`, `bd ready`, and `bd show determinism-hypervisor-5yo`
- Current M6 stack branches through `ralph/iteration-105-dhsnap-mcfg-decode`

Note: the requested `docs/phase2/gaps.md` path is not present in this checkout;
the matching gap assessment lives under the external `.agents/projects` tree.

## Review decision

Two requested subagent reviews completed:

- M6 implementation-risk review recommended starting with
  `determinism-hypervisor-ysm`, then a narrow `RestoreSnapshot` runtime-table
  population path.
- Dependency-order review recommended `determinism-hypervisor-5yo` first because
  M4 evidence is already closed, this M5 acceptance bead is ready, and it blocks
  M6 acceptance (`bik`) plus capture-neutrality acceptance (`pee`).

Decision: take `5yo` on a branch from `main`. The existing M6 branches are
stacked and valid for later `rfv` work, but `5yo` is the current critical-path
evidence gap from the dependency-order plan and is independent of that stack.

## Implementation

- Added `crates/dh-worker/tests/m5_frame_scheduling.rs`.
- The test boots the `fake_frames` nanokernel, runs to a `FrameBudget` stop,
  snapshots the paused boundary through the real in-process snapshot-store,
  restores into a fresh slot, then runs another `FrameBudget` segment.
- It verifies:
  - logged FRAME_MARK rows are strict and resolve `at_frame` by absolute
    `FRAME_COUNTER`;
  - first segment frames are `[1, 2, 3]`;
  - restored PADD state carries `FRAME_COUNTER == 3`;
  - post-restore frames continue as `[4, 5]`;
  - resolving frame `5` succeeds after restore while segment-relative frame `2`
    does not, proving the absolute-frame basis.

## Verification so far

- `cargo test -p dh-worker --test m5_frame_scheduling -- --nocapture` - passed
- `rustfmt --check crates/dh-worker/tests/m5_frame_scheduling.rs` - passed
- `cargo test -p dh-vmm frame_budget -- --nocapture` - passed
- `cargo test -p dh-worker --test m5_record_replay m5_smoke_record_replay_6s_vns_nonunit_clock -- --nocapture` - passed
- `cargo test -p dh-devices frame_counter_write_logs_frame_mark -- --nocapture` - passed
- `cargo test -p dh-devices snapshot_restore_roundtrip -- --nocapture` - passed
- `bash docs/ops/apply-host-config.sh --verify` - passed on this reference host
- `bash ci/check-determinism-class.sh` - passed; lock matches i5-8400 / microcode `0xfa` / kernel `6.8.0-124-generic`
- `cargo run -p dh-worker --bin dh-workerd -- --preflight` - passed
- `cargo check -p dh-worker` - passed
- `cargo test -p dh-worker` - passed

Known unrelated formatting drift:

- `cargo fmt --check -p dh-worker` fails on existing formatting in
  `crates/dh-worker/tests/perf_gates.rs`. The new test file is
  rustfmt-clean; `perf_gates.rs` was left untouched to keep this branch scoped.
- Follow-up filed as `determinism-hypervisor-b2a`.

## Remaining before close

- Push branch and Beads state.
