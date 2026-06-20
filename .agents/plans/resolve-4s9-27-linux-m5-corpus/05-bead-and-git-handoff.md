# Bead And Git Handoff

## Beads Workflow

Use Beads for task tracking.

Start:

```bash
bd show 4s9.27
bd update 4s9.27 --claim
```

If Beads refuses to claim a blocked issue, update notes instead and proceed only after confirming with `bd show 4s9.27` that the work is still assigned and visible.

Close only after the target Linux command and regression commands pass with `DH_M9_ALLOW_SKIP=0`.

Close evidence should include:

- host name/kernel;
- artifact paths and BLAKE3 hashes;
- `ci/determinism-class.lock` BLAKE3 hash;
- exact Linux M5 command;
- test name;
- frame budget and hard cap;
- DHILOG hash;
- record count;
- `epoch_hashes_verified`;
- END state hash;
- `VerifyReplay` Done hash;
- regression command list.

## Git Scope

Expected changed files:

- `crates/dh-worker/tests/m5_record_replay.rs`
- `crates/dh-worker/tests/common/mod.rs` if the `VerifyReplay` evidence helper is shared
- `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/README.md`
- `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/expected.txt`

Avoid unrelated docs changes in this branch. Downstream docs beads cover final Phase 1/Phase 2 evidence.

## Commit Shape

Suggested commit message:

```text
Add Linux M5 record replay corpus gate
```

If a guest-sdk fixture refresh is needed, make that a separate commit in `/home/infra-admin/git/preestablished/guest-sdk` and push it before the hypervisor evidence update. For this plan, no guest-sdk change is expected because the current staged M9 workload already emits frame marks and pv-blk IO.

## Mandatory Session Close

Follow project close protocol.

In `determinism-hypervisor`:

```bash
git status
git add <changed files>
git commit -m "Add Linux M5 record replay corpus gate"
git pull --rebase
bd dolt push
git push
git status
```

Final `git status` must say:

```text
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

## Expected Downstream Result

After closing `4s9.27`, run:

```bash
bd ready
bd blocked
```

Expected result:

- `4s9.29` should become actionable or at least have only non-`4s9.27` blockers.
- `4s9.32` remains blocked until `4s9.29` and `4s9.31` complete.

Do not silently start downstream work unless explicitly asked.
