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
| Device snapshots | `crates/dh-devices` and `crates/dh-snapshot` tests; [`docs/upstream-divergences.md`](upstream-divergences.md) | EVTC v2 is a 43-byte base plus variable-length pending InjectQuery entries; v1 remains a 39-byte restore-compatible legacy layout. NETL is 36 bytes of registers with no pending-RX state to serialize, and ENTR v2 is the snapshot-engine layout. |

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

## M9 post-Linux nanokernel preservation (2026-06-20)

**Host:** infra-control, Linux `6.8.0-124-generic`, Intel(R) Core(TM)
i5-8400 CPU @ 2.80GHz, microcode `0xfa`; `bash
ci/check-determinism-class.sh` reported all 7 lock keys matched. `/dev/kvm`
was present and rw, `nasm` was `/usr/bin/nasm`, and `taskset -c 2-5` reported
`Cpus_allowed_list: 2-5`.

This addendum preserves the pre-existing nanokernel M5/M7 coverage after M9
Linux work. The Linux rollup below completes the M9 Phase 2 exit-gate update
tracked by `determinism-hypervisor-4s9.32`.

| Command | Evidence |
|---|---|
| `cargo test --workspace` | PASS. The run included worker restore/fork/replay tests, nanokernel fixture drift tests, and the checked-in `pad_echo_6s` corpus reverify. |
| `cargo test -p dh-worker --test m5_record_replay record_replay_corpus_pad_echo_6s_reverifies -- --nocapture` | PASS: `record_replay_corpus_pad_echo_6s_reverifies ... ok`. |
| `cargo test -p dh-worker --test m7_fork_verify -- --nocapture` | PASS: non-ignored helper tests 3/3; the full acceptance and cross-slot M7 gates remained ignored and discoverable. |
| `DH_M7_ACCEPT_JOBS=2 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_1000_seeded_forks_verify_replay_all -- --ignored --nocapture` | PASS: `M7 fork/verify done: verified=2 divergence=0 unique_hashes=2`. `DH_M7_ACCEPT_GUEST` was unset, so the harness used the default nanokernel `pad_echo` fixture. |

M7 nanokernel operator commands remain documented in
[`docs/ops/test-partitioning.md`](ops/test-partitioning.md): the full
1000-child acceptance, the 100-child nightly canary, and the cross-slot rerun
determinism command are still separate nanokernel/default rows. Fixture
preservation checks before the documentation edits showed no changes under
`tests/nanokernel/**` or
`crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/**`.

## M9 Linux Phase 2 rollup (2026-06-20)

**Host:** infra-control, Linux `6.8.0-124-generic`, Intel(R) Core(TM)
i5-8400 CPU @ 2.80GHz, microcode `0xfa`. These Linux artifact-backed gates
require staged `DH_M9_*` artifacts, live KVM, and the `kvm-intel` host. Final
M9 Phase 2 evidence must use `DH_M9_ALLOW_SKIP=0` and, for M7, also
`DH_M7_ACCEPT_ALLOW_SKIP=0`; any `*_ALLOW_SKIP=1` run is guard-path evidence
only and is rejected for acceptance.

The accepted Linux M4/M5/M7 producer evidence used this artifact set:

| Artifact | BLAKE3 |
|---|---|
| `bzImage` | `595466463a37efac6822ffccf3e61d0a2230e7d223a94c0bce5eb78b2f43bee9` |
| `initramfs.cpio` | `87edf64db22dc85ef0c6b17fdc6e58a8f73391a6053e96f7a1056da7d08b9f57` |
| `base.img` | `488de202f73bd976de4e7048f4e1f39a776d86d582b7348ff53bf432b987fca8` |
| `game.img` | `e02849845005d9d34fa3245d98fa59116a0245ed0136b496dbd2defebdc203ac` |
| `m9-refwork-contract` workload | `5d5eef1996e2e05168d23f99c46ee56dbd3fe02806b37b089f973868d2271346` |

| Gate | Command | Evidence |
|---|---|---|
| Linux M4 snapshot/restore/fork transparency | `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m4_transparency --release linux -- --ignored --nocapture` | PASS with no skips: mid `icount=642343512`, `frame_counter=6`, state hash `f4a21f31c9f563163af5c69b528a5e66a0a8ccacd51c4d0a712718fd10a43928`; control/restored `icount=643343512`, `frame_counter=13`, state hash `e54386c97bffb09a898c7cf73b73cfeeff357d207fcfc83ef26f6dbc872e0ac3`; snapshot diff `reg_diffs=0`, `diff_pages=[]`. |
| Linux M5 frame scheduling | `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m5_frame_scheduling --release linux -- --ignored --nocapture` | PASS: first post-READY frame table `[(186992, 1), (330795, 2), (474598, 3)]`; restored frame table `[(143803, 4), (287606, 5)]`, proving frame-budget continuity across restore on the Linux fixture. |
| Linux M5 deterministic I/O loopback | `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture` | PASS: M9 ships without Linux pv-net, so the Linux filter uses a guest-driven pv-blk I/O loopback segment. Evidence: `run_icount=641530504`, `frame_counter=1`, meta proof checksum `0xcfe2fddd7d2669a3`, `blko_dirty_clusters=1`, and VerifyReplay reproduces the sealed segment. |
| Linux M5 post-READY record/replay corpus | `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test m5_record_replay --release linux_m5_record_replay_post_ready_corpus_reverifies -- --ignored --nocapture` | PASS against `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/expected.txt`. Manifest pins `ready_snapshot_ref=8996b67f3a6578062f8838a53bd72945567a525a0ac476a3366bfb3a3df6c088`, `end_snapshot_ref=725a09e6bd6456575c9aab50cfbf637f37853152a55f87d6f34af8072ce5f15e`, `dhilog_blake3=5a31d75e48dc52f52dca57e946a5099b7e14c33fb9f963a276073e511e7666c2`, `epoch_hashes_verified=1`, `end_state_hash=32454a811351f338fedacd0294358e442b5a84e849b63b721db3cfbd631ce5a1`, `frame_counter=5`, and meta pv-blk checksum `14979814438316960163`. |
| Linux worker API support | `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture` | PASS: 2 ignored Linux worker API tests passed, covering CreateVm BzImage, Run-to-Ready EventKind 14, StreamGuestEvents, ReadGuestMemory region ranges, TakeSnapshot, RestoreSnapshot, Fork, child Run, and VerifyReplay. This is supporting API evidence for the M9 Phase 2 gates. |
| Linux M7 full fork/VerifyReplay | `DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux DH_M7_ACCEPT_JOBS=1000 DH_M7_CROSS_CHECKS=10 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture --test-threads=1` | PASS in the unfiltered ignored run: 2 tests passed in 802.23s. Full fork test reached `M7 Linux fork/verify done: verified=1000 divergence=0 unique_hashes=1 epoch_hashes=1000`; every VerifyReplay stream reached Done and `Done.end_state_hash` matched the child snapshot state hash. The same run completed cross-slot samples at job indices 0, 111, 222, 333, 444, 555, 666, 777, 888, and 999. |
| Linux M7 cross-slot rerun determinism | `DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux DH_M7_ACCEPT_JOBS=1000 DH_M7_CROSS_CHECKS=10 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture` | PASS in 162.73s. The 10 sampled same-seed jobs matched child snapshot refs, state hashes, input log ids, DHILOG payloads, parsed end counters/frame marks, and meta I/O checksums across child slots. |
| Linux M7 nightly canary | `DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux DH_M7_ACCEPT_JOBS=100 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c "$DH_M7_ACCEPT_SLOT_CORES" cargo test -p dh-worker --test m7_fork_verify --release m7_accept_1000_seeded_forks_verify_replay_all -- --ignored --nocapture` | PASS as a nightly-equivalent 100-child Linux canary with `verified=100`, `divergence=0`, `unique_hashes=1`, and `epoch_hashes=100`. `.github/workflows/nightly-drift.yaml` schedules this as `m7-linux-fork-verify-100` beside the existing nanokernel/default `m7-fork-verify-100` canary. |

CI and nightly classification lives in
[`docs/ops/test-partitioning.md`](ops/test-partitioning.md) and
[`docs/ops/github-runner.md`](ops/github-runner.md): Linux fixture contract,
Ready, Phase 1 CLI, timer/IRQ, landing/counting, M4/M5, M5 corpus, worker API,
full Linux M7, and Linux cross-slot gates are operator-run acceptance commands
except for the scheduled 100-child Linux M7 nightly canary. The nanokernel
record/replay corpus reverify and nanokernel/default M7 nightly canary remain
separate coverage.

## M9 final acceptance suite (2026-06-21)

`determinism-hypervisor-4s9.35` was rerun end-to-end on infra-control before
closing M9. The tested code was commit
`f855dfb9800e969e8371016112aace7703ee402d`; later commits are docs-only
evidence publication. The Phase 1 final acceptance section records the local
transcript caveat, host, runner reservation, and four boot-image hashes. The
final suite did not materialize and rehash `m9-refwork-contract` as a separate
file; the Linux fixture-contract test validated the in-initramfs exec path and
expected regions. All rows below used no-skip acceptance settings:
`DH_M9_ALLOW_SKIP=0` for Linux gates and `DH_M7_ACCEPT_ALLOW_SKIP=0` for M7
acceptance gates. Filtered ignored-test transcripts showed the named tests and
nonzero passed counts; no `*_ALLOW_SKIP=1` evidence is used for acceptance.
Where hashes differ from older M9 Phase 2 rollup rows above, this section is
the final 2026-06-21 acceptance record for `determinism-hypervisor-4s9.35`;
the earlier rows remain historical producer evidence.

| Gate | Command | Evidence |
|---|---|---|
| Linux M4 snapshot/restore/fork transparency | `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m4_transparency --release linux -- --ignored --nocapture` | PASS: mid `icount=642343512`, `vns=642343512`, `frame_counter=6`, state hash `c6e320337fd0d1c208ab76ed411989c2ca838908fea96a0833b097c1ff4350d4`; control/restored `icount=643343512`, `vns=643343512`, `frames=7`, `frame_counter=13`, state hash `201276c8bd969dbdbfb9fd1ae11f43b455f0569c959b8f1302dbc872d644c87b`; restored-mid hash matched mid, restored hash matched control, `rip_expected=0x7ffff7f9a5d8`, `rip_actual=0x7ffff7f9a5d8`, `reg_diffs=0`, `diff_pages=[]`; `1 passed`, no skips accepted. |
| Linux M5 frame scheduling | `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m5_frame_scheduling --release linux -- --ignored --nocapture` | PASS: `linux-m5 frames start=0`, `first_icount=641818110`, first frames `[(186992, 1), (330795, 2), (474598, 3)]`, `restored_icount=642105716`, restored frames `[(143803, 4), (287606, 5)]`; `1 passed`, no skips accepted. |
| Linux M5 deterministic I/O loopback | `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture` | PASS: `linux-pvblk-io run_icount=641530504`, `frame_counter=1`, checksum `0xcfe2fddd7d2669a3`, `blko_dirty_clusters=1`; `1 passed`, no skips accepted. |
| Linux M5 post-READY record/replay corpus | `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test m5_record_replay --release linux_m5_record_replay_post_ready_corpus_reverifies -- --ignored --nocapture` | PASS: current M9 artifacts matched `bzImage=595466463a37efac6822ffccf3e61d0a2230e7d223a94c0bce5eb78b2f43bee9`, `initramfs=87edf64db22dc85ef0c6b17fdc6e58a8f73391a6053e96f7a1056da7d08b9f57`, `base=488de202f73bd976de4e7048f4e1f39a776d86d582b7348ff53bf432b987fca8`, `game=e02849845005d9d34fa3245d98fa59116a0245ed0136b496dbd2defebdc203ac`; `frames=5`, `hard_cap=5000000`, `end_icount=762204`, `epochs=1`, `end_state_hash=32454a811351f338fedacd0294358e442b5a84e849b63b721db3cfbd631ce5a1`, checksum `0xcfe2fddd7d2669a3`, `dhilog=5a31d75e48dc52f52dca57e946a5099b7e14c33fb9f963a276073e511e7666c2`; `1 passed`, no skips accepted. |
| Linux worker API support | `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture` | PASS: `2 passed`, no skips accepted, covering the Linux worker API path through CreateVm, Run, Snapshot, Restore, Fork, and VerifyReplay. |
| Nanokernel M5 record/replay corpus | `cargo test -p dh-worker --test m5_record_replay record_replay_corpus_pad_echo_6s_reverifies -- --nocapture` | PASS: existing nanokernel corpus reverified; `1 passed`. |
| Nanokernel M7 full fork/VerifyReplay | `env -u DH_M7_ACCEPT_GUEST DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture --test-threads=1` | PASS: `M7 fork/verify done: verified=1000 divergence=0 unique_hashes=1000`; cross-slot samples completed at job indices 0, 111, 222, 333, 444, 555, 666, 777, 888, and 999; `2 passed`, no skips accepted. |
| Nanokernel M7 cross-slot rerun determinism | `env -u DH_M7_ACCEPT_GUEST DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture` | PASS: cross-slot progress reached `10/10` at job indices 0, 111, 222, 333, 444, 555, 666, 777, 888, and 999; `1 passed`, no skips accepted. |
| Linux M7 full fork/VerifyReplay | `DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux DH_M7_ACCEPT_JOBS=1000 DH_M7_CROSS_CHECKS=10 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture --test-threads=1` | PASS: `M7 Linux fork/verify done: verified=1000 divergence=0 unique_hashes=1 epoch_hashes=1000`; cross-slot samples completed at job indices 0, 111, 222, 333, 444, 555, 666, 777, 888, and 999; `2 passed`, no skips accepted. |
| Linux M7 cross-slot rerun determinism | `DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux DH_M7_ACCEPT_JOBS=1000 DH_M7_CROSS_CHECKS=10 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture` | PASS: Linux cross-slot progress reached `10/10` at job indices 0, 111, 222, 333, 444, 555, 666, 777, 888, and 999; `1 passed`, no skips accepted. |

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
