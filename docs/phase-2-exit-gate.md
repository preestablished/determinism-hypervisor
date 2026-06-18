# Phase 2 exit gate -- sign-off record (bead l0h)

**Date:** 2026-06-16. **Host:** infra-control (i5-8400, kernel
6.8.0-124-generic, microcode 0xfa -- `ci/determinism-class.lock`).
The validation rows below were checked for this sign-off on the same
determinism-class host. This shell is restricted to housekeeping CPUs
0-1, so the full M7 slot-core acceptances remain the operator-run
commands in [`docs/ops/test-partitioning.md`](ops/test-partitioning.md)
for the isolated 2-5 slot-core affinity.

This is the Phase-2 close-out record for snapshot, fork, and replay in
this repo. With it closed, the implementation work in
`determinism-hypervisor` has an as-built trail for the engines, the
frozen binary formats, the measured perf gates, and the ops commands
that prove the branch remains host-runnable.

**Scope:** this repo owns the VMM/worker behavior, DHSNAP and DHILOG
codecs, snapshot/restore/fork engines, VerifyReplay, the M6 daemon API
surface, and the M7 fork/replay harnesses. The sibling repos remain
authoritative for their own surfaces: `snapshot-store` owns store
durability, crash-injection, and storage-service internals; `guest-sdk`
owns in-guest agent APIs and event encoders; `control-plane` owns the
canonical proto schema consumed through the local proto seam. Those
boundaries are out of scope here by ownership, not omission.

## As-built architecture notes

- **Snapshot and restore:** `dh-snapshot` is a byte-level DHSNAP v1.0
  container codec with golden fixtures. Worker snapshots serialize RAM
  pages plus a DHSNAP device blob into the real snapshot-store client
  path. Restore loads pages into a fresh slot, decodes DHSNAP, restores
  vCPU/device/time/entropy state, and returns the `MCFG` machine config
  decoded from the snapshot. TSC alignment uses
  `KVM_VCPU_TSC_OFFSET`, per
  [`docs/decisions/tsc-alignment.md`](decisions/tsc-alignment.md).
- **Store fixture:** host-runnable tests spawn the real
  `snapstore-server` in a TempDir over UDS, rather than a mock store.
  That fixture is the accepted local integration seam for this repo
  ([decision](decisions/snapstore-server-for-tests.md)).
- **Dirty tracking:** the engine uses KVM dirty-ring harvest for
  incremental snapshots. The accepted chaos floor is the smallest legal
  ring on the lab kernel, 1024 entries, because 512 is rejected by KVM
  on this box.
- **Fork:** tier-A fork starts from a frozen parent and creates children
  through CoW memory plus in-memory DHSNAP restore. A parent cannot run
  while children live; a child is not a new fork parent. The service
  opens a fresh DHILOG segment for each child. Child entropy continues
  from the fork-point ENTR state unless the caller supplies an explicit
  nonzero segment seed; the M7 harness supplies explicit per-child
  seeds.
- **Replay and VerifyReplay:** DHILOG v1 records the segment between
  snapshots, including injected inputs, entropy draws, SDK events, epoch
  hashes, and stop reason. VerifyReplay consumes a base snapshot plus
  DHILOG, streams epoch progress/divergence through the worker API, and
  validates the end hash. Lineage splicing composes segment chains
  without changing the frozen DHILOG v1 record format.
- **M6/M7 ops:** the daemon exposes the v1 worker gRPC surface through
  the sibling proto seam
  ([decision](decisions/proto-seam.md)). The M6 grpcurl/metrics smoke is
  documented in [`docs/ops/m6-grpcurl-metrics-smoke.md`](ops/m6-grpcurl-metrics-smoke.md).
  M7 coverage is split into the full 1000-child acceptance, a nightly
  100-child canary, cross-slot rerun determinism, and the throughput
  soak under housekeeping load.

## Frozen formats and fixtures

| Format | Freeze anchor | As-built notes |
|---|---|---|
| DHSNAP v1.0 | `crates/dh-snapshot/tests/golden.rs`; `crates/dh-snapshot/tests/fixtures/v1_minimal.dhsnap`, `v1_kitchen_sink.dhsnap`, `v1_entr_v2.dhsnap` | Header and section layout are BLAKE3-pinned. Layout changes require a format bump and new fixture names. ENTR v2 is 72 bytes: the v1 ChaCha20 state plus pv-entropy guest-visible registers. |
| DHILOG v1.0 | `crates/dh-inputlog/tests/golden.rs`; `crates/dh-inputlog/tests/fixtures/v1_minimal.dhilog`, `v1_kitchen_sink.dhilog`; `crates/dh-inputlog/tests/reader_validation.rs`; `crates/dh-inputlog/src/splice.rs` | Header, writer-emitted record kinds, encoder fingerprint field, and END stop-reason byte are pinned by checked-in bytes. NET_RX lower-bound validation is covered by reader validation, and lineage splicing is covered by splice tests without changing the frozen DHILOG v1 record format. |
| Record/replay corpus | `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s` | Nightly drift replays the corpus so behavioral drift in code, kernel, or microcode has a named failure surface. |
| Device snapshots | `crates/dh-devices` and `crates/dh-snapshot` tests; [`docs/upstream-divergences.md`](upstream-divergences.md) | EVTC v1 is 39 bytes, NETL is 36 bytes of registers with no pending-RX state to serialize, and ENTR v2 is the snapshot-engine layout. |

## Measured perf and ops numbers

The M4 perf surfaces are reference-machine telemetry, not hard latency
acceptance gates. The authority is
`crates/dh-worker/tests/perf_gates.rs` and
[`docs/upstream-divergences.md`](upstream-divergences.md) ledgers #20
and #21. Slow storage is acceptable on the reference Linux KVM machine
as long as snapshot, restore, fork, replay, and durable store semantics
remain deterministic and correct.

The table keeps both the original accepted-as-measured baseline and the
2026-06-17 reference-host observation that triggered bead 3sp's policy
update:

| Operation | 2026-06-12 p50 | 2026-06-17 p50 | Acceptance |
|---|---:|---:|---|
| Tier-A fork of a frozen 128 MiB parent | 326 us | 1.895 ms | Completes correctly; latency is telemetry |
| Incremental snapshot, 8192 dirty 4 KiB pages | 103 ms | 355 ms | Ships exactly 8192 pages; latency is telemetry |
| Tier-B warm restore of a 128 MiB root | 307 ms | 1.528 s | Loads exactly 32768 pages; latency is telemetry |

The real store gives durability receipts and the box's durable storage
bandwidth is the bottleneck. The original 15 ms snapshot, 150 ms
restore, and later accepted 150 ms / 450 ms storage caps are retained as
historical context only. They are not Phase-2 correctness criteria.

Ops tooling is pinned in
[`docs/ops/github-runner.md`](ops/github-runner.md): `grpcurl` for the
M6 smoke, `cargo-fuzz` plus nightly Rust for DHILOG fuzzing, and
`stress-ng` for the M7 throughput soak. The soak script requires
non-overlapping slot and housekeeping CPU masks by default.

## Gate record

| # | Gate | Evidence |
|---|---|---|
| 1 | Workspace non-ignored suite remains green | `cargo test --workspace`: PASS on 2026-06-16 after checkpoint commit `089d9eb` |
| 2 | Workspace build remains green | `cargo build --workspace`: PASS on 2026-06-16 after checkpoint commit `089d9eb` |
| 3 | M4/M5 format freezes are present | DHSNAP and DHILOG golden tests are part of the workspace suite; fixture bytes are checked in and BLAKE3-pinned |
| 4 | Real store fixture is documented and exercised | `cargo test -p determinism-tests --test store_joint` runs through `snapstore-server` over UDS and is part of the workspace suite |
| 5 | M6 daemon/ops surface is documented | [`docs/ops/m6-grpcurl-metrics-smoke.md`](ops/m6-grpcurl-metrics-smoke.md) covers grpcurl calls, health, metrics, snapshot, restore, fork, VerifyReplay, and framebuffer read probes |
| 6 | M7 fork/VerifyReplay harness remains buildable and discoverable in this constrained shell | `cargo test -p dh-worker --test m7_fork_verify -- --nocapture` covers non-ignored helper tests, and `cargo test -p dh-worker --test m7_fork_verify --release --no-run` compiles the ignored acceptance target on 2026-06-16 |
| 7 | M7 full slot-core gates are preserved as operator commands | Full acceptance, 100-child nightly canary, cross-slot rerun determinism, and throughput soak commands are listed in [`docs/ops/test-partitioning.md`](ops/test-partitioning.md). This shell only exposes CPUs 0-1, so `DH_M7_ACCEPT_SLOT_CORES=0-1 DH_M7_ACCEPT_ALLOW_SKIP=1 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture` confirms the guard path but does not replace the 2-5 slot-core operator run. |
| 8 | Runner/tooling state recorded | [`docs/ops/github-runner.md`](ops/github-runner.md) records runner identity, fork-PR security policy, slot-core isolation, and pinned tool versions for M6/M7 |

## M9 pre-Linux baseline refresh (2026-06-18)

**Host:** infra-control, Linux `6.8.0-124-generic`, Intel(R) Core(TM)
i5-8400 CPU @ 2.80GHz, microcode `0xfa`; live values match
`ci/determinism-class.lock`.

Before M9 Linux edits, the current nanokernel and worker baselines were
rerun or re-recorded on this host:

| Command | Evidence |
|---|---|
| `cargo test --workspace` | PASS on 2026-06-18. The run included the Phase 1 KVM tests (`if0_deferral`, `landing_precision`, `m1_acceptance`, `regression`, `timer_determinism`), VMM live tests, worker restore/fork/replay tests, and nanokernel fixture drift tests. |
| M5 corpus reverify | `crates/dh-worker/tests/m5_record_replay.rs`: `record_replay_corpus_pad_echo_6s_reverifies ... ok` during `cargo test --workspace`. The explicit rebaseline test remained ignored with the documented guard `DH_WORKER_REGEN_RR_CORPUS=1 cargo test -p dh-worker --test m5_record_replay regenerate_record_replay_corpus_pad_echo_6s -- --ignored --nocapture`. |
| M5 long acceptance | Still host/operator-gated and ignored by default: `m5_accept_record_replay_60s_vns_pad_sequence_x100 ... ignored`, command `cargo test -p dh-worker --test m5_record_replay --release -- --ignored --nocapture`. |
| M6 slot-core acceptance | Still host/operator-gated and ignored by default: `m6_full_api_uds_64_concurrent_slots_match_single_slot_baseline ... ignored`, command `cargo test -p dh-worker --test m6_full_api_uds --release -- --ignored --nocapture`. |
| M7 full fork/VerifyReplay acceptance | Target discovered; helper test `cross_check_indices_cover_the_1000_job_universe ... ok`. Full operator command remains `cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture`. |
| M7 cross-slot rerun acceptance | Target discovered and ignored by default. Operator command remains `DH_M7_ACCEPT_SLOT_CORES=2-5 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture`. |
| M4 perf telemetry | Still ignored by default: `m4_perf_gates_p50_128mib ... ignored`, command `cargo test -p dh-worker --test perf_gates --release -- --ignored --nocapture`. |

## Known refinements baked into the gate

- Snapshot and restore perf numbers are storage telemetry on the
  reference machine; correctness and durable receipts outrank latency.
- Dirty-ring chaos uses the 1024-entry legal minimum on this kernel, not
  the originally sketched 512-entry ring.
- NETL has no pending-RX bytes by construction: TX is drained per exit
  and RX delivery is immediate at record landing.
- The state-hash vCPU preimage is field-selective and padding-excluded;
  it is not the raw DHSNAP VCPU section bytes.
- The M7 nightly is intentionally scaled to 100 children. The full
  1000-child fork/VerifyReplay command is the operator-run acceptance
  gate, and the throughput soak is kept out of scheduled nightly CI.

## What this unblocks

Phase 2 can be treated as closed in this repo. Downstream integration can
consume the as-built contracts for snapshot refs, fork lineage, DHILOG
segments, VerifyReplay, and ops runbooks while sibling repos continue
their own backlog under their own ownership boundaries.
