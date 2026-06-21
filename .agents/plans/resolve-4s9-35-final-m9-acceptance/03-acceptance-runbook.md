# Acceptance Runbook

Run commands from the repository root. Use a shell script or transcript file
to preserve command output, but run the gates sequentially. Do not use shell
chains that continue after a failure.

Recommended transcript setup:

```bash
evidence_dir="target/m9-final-acceptance-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$evidence_dir"
tested_code_sha=$(git rev-parse HEAD)
echo "tested_code_sha=$tested_code_sha" | tee "$evidence_dir/00-tested-code-sha.txt"
```

For each command, run it as a separate command and capture output with `tee`,
for example:

```bash
cargo test --workspace 2>&1 | tee "$evidence_dir/01-workspace.log"
test "${PIPESTATUS[0]}" -eq 0
```

If using `zsh`, use `${pipestatus[1]}` instead of Bash `PIPESTATUS`. The
important rule is that `tee` must not hide a failing cargo exit code.

For every filtered or ignored test command, the transcript must show the
expected named test and a nonzero pass count. Reject any transcript that says
`0 tests` or only proves a filter compiled.

## 1. Host And Fixture Preflights

```bash
bash docs/ops/apply-host-config.sh --verify
bash ci/check-determinism-class.sh
cargo run -p dh-worker --bin dh-workerd -- --preflight
DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_fixture_contract -- --ignored --nocapture
DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_ready --release -- --ignored --nocapture
```

Required evidence:

- `/dev/kvm` is usable.
- determinism class matches `ci/determinism-class.lock`.
- Linux fixture contract passes against the staged reference-workload initramfs.
- Linux READY is EventKind 14 on detchannel, not serial-only readiness.

## 2. Baseline Workspace And Nanokernel Phase 1

```bash
cargo test --workspace
cargo run -p dh-cli -- gate --runs 100
```

Required evidence:

- workspace non-ignored suite passes;
- default `dh-cli gate` remains nanokernel/default, not Linux;
- Phase 1 default gate reports 100/100 zero divergence.

## 3. Linux Phase 1 Final Gates

```bash
DH_M9_ALLOW_SKIP=0 \
cargo run -p dh-cli -- gate --linux --runs 100 \
  --bzimage "$DH_M9_BZIMAGE" \
  --initramfs "$DH_M9_INITRAMFS" \
  --base-image "$DH_M9_BASE_IMAGE" \
  --game-image "$DH_M9_GAME_IMAGE"
```

```bash
DH_M9_ALLOW_SKIP=0 \
cargo test -p determinism-tests --test linux_timer_determinism --release -- --ignored --nocapture
```

```bash
DH_M9_ALLOW_SKIP=0 \
cargo test -p determinism-tests --test linux_landing_counting --release -- --ignored --nocapture
```

Required evidence:

- Linux Phase 1 CLI gate passes 100 runs with zero divergence.
- timer/IRQ determinism passes 100 cold Linux cases.
- landing/counting passes 100 exact post-READY targets.
- The final transcript includes Ready identity, config hash, post-READY hash,
  timer vector/source/deadline metadata, and target hash range where the tests
  print them.

## 4. Linux M4 And M5 Final Gates

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
cargo test -p dh-worker --test m4_transparency --release linux -- --ignored --nocapture
```

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
cargo test -p dh-worker --test m5_frame_scheduling --release linux -- --ignored --nocapture
```

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture
```

```bash
DH_M9_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m5_record_replay --release \
  linux_m5_record_replay_post_ready_corpus_reverifies \
  -- --ignored --nocapture
```

Required evidence:

- M4 snapshot/restore/fork transparency passes with no register or page diffs.
- M5 frame scheduling preserves frame-budget continuity across restore.
- Linux `m5_net_loopback` passes as the accepted guest-driven pv-blk I/O
  substitute, not as Linux pv-net.
- M5 Linux corpus reverify passes and verifies at least one epoch hash.

## 5. Linux Worker API Final Gate

```bash
DH_M9_ALLOW_SKIP=0 \
cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture
```

Required evidence:

- CreateVm BzImage path is exercised.
- Run-to-READY reaches EventKind 14.
- StreamGuestEvents and ReadGuestMemory cover expected regions.
- TakeSnapshot, RestoreSnapshot, Fork, child Run, and VerifyReplay all pass.
- `MachineConfig.base_image_hash` remains the `DH_M9_GAME_IMAGE` hash, with
  `DH_M9_BASE_IMAGE` treated as fixture context.

## 6. Nanokernel M5 And M7 Preservation

```bash
cargo test -p dh-worker --test m5_record_replay \
  record_replay_corpus_pad_echo_6s_reverifies \
  -- --nocapture
```

```bash
env -u DH_M7_ACCEPT_GUEST \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
taskset -c 2-5 \
cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture --test-threads=1
```

```bash
env -u DH_M7_ACCEPT_GUEST \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
taskset -c 2-5 \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs \
  -- --ignored --nocapture
```

Required evidence:

- checked-in `pad_echo_6s` corpus still reverifies;
- default/nanokernel full M7 fork/VerifyReplay passes;
- default/nanokernel cross-slot same-seed rerun passes.
- transcripts show the nanokernel/default path, not `DH_M7_ACCEPT_GUEST=linux`.

## 7. Linux M7 Final Gates

Run these when the host is otherwise quiet. Use `taskset -c 2-5` exactly as
the Linux operator commands document.

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M7_ACCEPT_GUEST=linux \
DH_M7_ACCEPT_JOBS=1000 \
DH_M7_CROSS_CHECKS=10 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
taskset -c 2-5 \
cargo test -p dh-worker --test m7_fork_verify --release \
  -- --ignored --nocapture --test-threads=1
```

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M7_ACCEPT_GUEST=linux \
DH_M7_ACCEPT_JOBS=1000 \
DH_M7_CROSS_CHECKS=10 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
taskset -c 2-5 \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs \
  -- --ignored --nocapture
```

Required evidence:

- 1000 Linux fork children verify with zero divergence.
- `unique_hashes=1` and `epoch_hashes=1000` for the full Linux run.
- every VerifyReplay stream reaches Done;
- every `Done.end_state_hash` matches the child snapshot state hash;
- the 10 sampled same-seed cross-slot jobs match snapshot refs, state hashes,
  input log ids, DHILOG payloads, parsed end counters/frame marks, and meta
  I/O checksums.
- transcripts show the named test functions ran and did not report `0 tests`.

## 8. Optional Nightly-Equivalent Canary

This does not replace the full Linux M7 gate, but it is useful if final notes
need to connect local evidence to the scheduled nightly canary:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M7_ACCEPT_GUEST=linux \
DH_M7_ACCEPT_JOBS=100 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
taskset -c "$DH_M7_ACCEPT_SLOT_CORES" \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_1000_seeded_forks_verify_replay_all \
  -- --ignored --nocapture
```

If a recent GitHub nightly run is used as supporting evidence, capture its URL
or run id in the bead comment. Do not use the 100-child canary as the primary
closeout for `4s9.35`.
