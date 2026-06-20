# Beads And Handoff

## Beads Workflow

This repository uses `bd` for task tracking.

Before implementation:

```bash
bd prime
bd show determinism-hypervisor-4s9.29
bd update determinism-hypervisor-4s9.29 --claim
```

If `bd` refuses to claim a blocked issue, add a comment instead:

```bash
bd comment determinism-hypervisor-4s9.29 "Starting Linux M7 implementation now that 4s9.21, 4s9.27, and 4s9.28 are closed. Plan: .agents/plans/resolve-4s9-29-linux-m7-acceptance/"
```

Do not close `4s9.29` until the full Linux acceptance and cross-slot commands pass on the reference host with `DH_M9_ALLOW_SKIP=0`.

## Close Evidence To Record

When closing the bead, include:

- exact commit SHA;
- host name and determinism-class lock status;
- artifact hashes;
- exact full Linux acceptance command;
- final `1000/1000 VerifyReplay Done` evidence;
- zero `Divergence` statement;
- cross-slot command and sampled index count;
- nightly workflow change summary;
- documentation change summary.

Recommended close comment shape:

```text
Linux M7 acceptance complete on <host>.

Full:
<command>
Result: 1000/1000 VerifyReplay Done, zero Divergence, all Done.end_state_hash values matched child snapshot state hashes.

Cross-slot:
<command>
Result: <n> sampled indices, same-seed snapshot refs/state hashes/input log ids/DHILOG payloads matched across child slots.

Nightly:
<summary of m7-linux-fork-verify-100 job>

Artifacts:
bzImage=<hash>
initramfs=<hash>
base=<hash>
game=<hash>
```

## Required Session Closeout

AGENTS.md requires pushing code, Beads data, and git refs before ending a work session.

After implementation and validation:

```bash
git status --short --branch
cargo fmt --check
git diff --check
git add crates/dh-worker/tests/m7_fork_verify.rs \
  crates/dh-worker/tests/common/mod.rs \
  .github/workflows/nightly-drift.yaml \
  docs/ops/test-partitioning.md
git commit -m "Add Linux M7 fork VerifyReplay acceptance"
commit_sha=$(git rev-parse HEAD)
bd show determinism-hypervisor-4s9.29
bd comment determinism-hypervisor-4s9.29 "<paste close evidence here, including commit $commit_sha, host, artifact hashes, 1000/1000 VerifyReplay Done, zero Divergence, cross-slot result, nightly change, and docs change>"
bd close determinism-hypervisor-4s9.29
git pull --rebase
bd dolt push
git push
git status
```

`git status` must show the branch up to date with origin before handoff.

If the final implementation touches additional files, include them in the `git add` step. The important ordering is: validate, commit code/docs, record evidence on the bead with the exact commit SHA, close the bead, push Beads, then push git.

## Downstream Handoff

After closing `4s9.29`, run:

```bash
bd ready
```

Expected downstream work to re-evaluate:

- `4s9.31` should be ready or closer to ready because Linux M7 and nanokernel preservation can be checked together.
- `4s9.32` can collect Phase 1 and Phase 2 exit-gate evidence.
- `4s9.33` can finalize operator docs and CI/nightly classification.

Do not bundle those downstream beads into the `4s9.29` implementation unless explicitly asked. Keep this work scoped to Linux M7 acceptance and nightly canary wiring.
