# Beads And Closeout

## Start

Run Beads commands serially; embedded Dolt can lock on concurrent access.

```bash
bd show determinism-hypervisor-3l2
```

If the audit confirms all dependencies are closed and no external blocker
remains, move the parent back to open and claim it:

```bash
bd update determinism-hypervisor-3l2 --status open \
  --append-notes "Parent closeout audit started: all seven local unblock beads are closed; verifying current code/tests on the Linux/KVM reference host before closing M8 bisection diagnostics."
bd update determinism-hypervisor-3l2 --claim
```

If the audit finds real missing work, keep the bead open/in progress while
fixing it. If a non-local blocker appears, update the notes with the exact
blocker and leave the bead blocked.

## Expected Git Changes

If the audit passes without code changes, expected changes are only:

- `.agents/plans/resolve-3l2-bisection-diagnostics-closeout/`
- Beads state

If a gap is found, likely code files are:

- `crates/dh-worker/src/bisection_index.rs`
- `crates/dh-worker/src/replay_engine.rs`
- `crates/dh-worker/src/service.rs`
- `crates/dh-worker/src/snapshot_compare.rs`
- `tools/dh-cli/src/ops.rs`

Do not stage unrelated target artifacts, M9 artifacts, snapstore data, or image
cache entries.

## Closeout Comment Template

Before final validation, rebase once so the evidence is against the branch that
will be pushed:

```bash
git pull --rebase
```

If the rebase changes any code relevant to bisection, rerun the audit and
focused validation.

Stage and commit the intended files before posting closeout evidence:

```bash
git status --short --branch
git add <intended files>
git commit -m "Close out VerifyReplay bisection diagnostics"
commit_sha=$(git rev-parse HEAD)
```

Use the actual commit SHA from `git rev-parse HEAD` in the Beads comment. If a
later rebase changes the SHA, update the Beads comment or add a follow-up
comment with the final pushed SHA.

After validation passes and the final commit SHA is known, comment:

```bash
bd comment determinism-hypervisor-3l2 --stdin <<'EOF'
Resolved parent M8 VerifyReplay bisection diagnostics blocker.

Audit:
- 3l2.1 through 3l2.7 are closed.
- VerifyReplay bisect=true/default uses recorded BISECTION_CHECKPOINT evidence.
- Checkpoint-less bisect=true fails closed instead of fabricating diagnostics.
- bisect=false still returns coarse Divergence.
- Public Divergence fields are populated from snapshot comparison evidence:
  icount range, RIPs, postcard reg_diff, diff_page_idx, and provenance.

Validation:
- <focused commands>
- cargo fmt --check
- reference-host KVM/preflight/determinism-class checks
- cargo clippy --workspace --all-targets -- -D warnings (if code changed)
- cargo build --workspace (if code changed)
- cargo test --workspace
- <Linux/KVM reference-host command, if run>

Commit: <sha>
EOF
```

Then close:

```bash
bd close determinism-hypervisor-3l2 \
  --reason "VerifyReplay divergence bisection is evidence-backed by recorded checkpoints, tested at service/CLI surfaces, and validated on the Linux/KVM reference host."
```

## Required Push Sequence

Repository instructions require Beads and Git state to be pushed before
handoff:

```bash
git status --short --branch
bd dolt push
git push
git status --short --branch
```

The final status must be clean and up to date with origin.

If push fails, resolve and retry rather than leaving the closeout local.
