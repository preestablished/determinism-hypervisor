# 4s9.31 Nanokernel Preservation

## Objective

Prove that M9 Linux work did not remove, rewrite, or weaken existing nanokernel regression coverage. Close `4s9.31` only after the nanokernel evidence is fresh and published.

## Files To Inspect

- `docs/phase-1-exit-gate.md`
- `docs/phase-2-exit-gate.md`
- `docs/ops/test-partitioning.md`
- `tests/nanokernel/**`
- `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/**`
- `crates/dh-worker/tests/m5_record_replay.rs`
- `crates/dh-worker/tests/m7_fork_verify.rs`
- `tools/dh-cli/src/gate.rs`

Do not edit `tests/nanokernel/**` or existing corpus fixture bytes. If a command indicates they must change, file a separate bead instead of making the fixture change under `4s9.31`.

## Required Evidence Commands

Run on the KVM reference host. Keep long KVM commands serial.

```bash
git status --short --branch
bash ci/check-determinism-class.sh
cargo test --workspace
cargo run -p dh-cli -- gate --runs 100
cargo test -p determinism-tests --test regression --test timer_determinism --test if0_deferral --test landing_precision --test counting_semantics --test counting_smoke --test m1_acceptance
cargo test -p dh-worker --test m5_record_replay record_replay_corpus_pad_echo_6s_reverifies -- --nocapture
cargo test -p dh-worker --test m7_fork_verify -- --nocapture
```

Also run at least a small real nanokernel M7 acceptance smoke to prove the ignored path still uses nanokernel by default:

```bash
DH_M7_ACCEPT_JOBS=2 \
DH_M7_ACCEPT_SLOT_CORES=0-1 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_1000_seeded_forks_verify_replay_all \
  -- --ignored --nocapture
```

If the shell exposes cores `2-5` directly or via `taskset`, prefer the documented slot cores:

```bash
DH_M7_ACCEPT_JOBS=2 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_1000_seeded_forks_verify_replay_all \
  -- --ignored --nocapture
```

Do not set `DH_M7_ACCEPT_GUEST=linux` for this nanokernel preservation smoke.

## Fixture Integrity Checks

Before and after any doc edits, check that nanokernel fixture paths are unchanged:

```bash
git diff --exit-code -- tests/nanokernel crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s
test -z "$(git status --porcelain=v1 -- tests/nanokernel crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s)"
```

Expected result: no changes.

## Documentation Updates

Update `docs/phase-1-exit-gate.md` and `docs/phase-2-exit-gate.md` with a dated post-M9 nanokernel preservation section.

Keep this scoped to `4s9.31`: add only a dated nanokernel-preservation addendum. Do not publish Linux exit-gate evidence, update the full Phase 1/Phase 2 Linux gate tables, or claim `4s9.32` acceptance from this bead. If a doc already has a broader section reserved for `4s9.32`, point to the new nanokernel addendum rather than folding Linux evidence into `4s9.31`.

Include:

- date;
- host name;
- kernel/microcode and determinism-class status;
- exact commands run;
- pass/fail summaries;
- statement that `dh-cli gate --runs 100` still defaults to nanokernel;
- statement that M5 nanokernel corpus reverify still passes;
- statement that M7 nanokernel operator commands remain documented;
- statement that `tests/nanokernel/**` and existing corpus fixtures were not changed.

Do not rewrite old evidence sections as if they were fresh. Add a new dated section, or clearly amend the current M9 preservation section with the new date and commands.

## Close Criteria

Close `4s9.31` only when:

- all required evidence commands pass or any omitted long command is explicitly justified by the bead owner;
- docs contain fresh nanokernel preservation evidence only, without claiming the downstream `4s9.32` Linux-plus-nanokernel exit-gate update;
- fixture integrity checks show no nanokernel fixture/corpus changes;
- `git diff --check` and `cargo fmt --check` pass if any Markdown or Rust files changed.
