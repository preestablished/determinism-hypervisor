# Closeout And Handoff

## Commit Strategy

Prefer one focused commit for both beads only if both are completed in the same session. If `4s9.31` and `4s9.33` are completed separately, use separate commits:

```bash
git add docs/phase-1-exit-gate.md docs/phase-2-exit-gate.md docs/ops/test-partitioning.md
git commit -m "Publish post-M9 nanokernel preservation evidence"
```

```bash
git add docs/ops/test-partitioning.md docs/ops/github-runner.md .github/workflows/ci.yaml .github/workflows/nightly-drift.yaml
git commit -m "Document M9 Linux gate classification"
```

Adjust staged files to the actual diff. Do not stage unrelated working-tree changes.

## Close 4s9.31

Before recording final evidence, rebase so the commit SHA in Beads is the SHA that will be pushed:

```bash
git pull --rebase
commit_sha=$(git rev-parse HEAD)
echo "Evidence commit: $commit_sha"
bd comment determinism-hypervisor-4s9.31 --stdin <<'EOF'
<paste evidence from 05-validation-and-evidence.md, including the evidence commit printed above>
EOF
bd close determinism-hypervisor-4s9.31 --reason="Post-M9 nanokernel preservation evidence published"
```

Then check what unblocks:

```bash
bd ready
bd show determinism-hypervisor-4s9.32
```

Do not start `4s9.32` unless the user asks.

## Close 4s9.33

Before recording final evidence, rebase so the commit SHA in Beads is the SHA that will be pushed:

```bash
git pull --rebase
commit_sha=$(git rev-parse HEAD)
echo "Evidence commit: $commit_sha"
bd comment determinism-hypervisor-4s9.33 --stdin <<'EOF'
<paste evidence from 05-validation-and-evidence.md, including the evidence commit printed above>
EOF
bd close determinism-hypervisor-4s9.33 --reason="Linux gate commands, runner requirements, and CI/nightly classification documented"
```

Then check what unblocks:

```bash
bd ready
bd show determinism-hypervisor-4s9.34
bd show determinism-hypervisor-4s9.35
```

Do not start `4s9.34` or `4s9.35` unless the user asks.

## Mandatory Repo Closeout

AGENTS.md requires pushed code and Beads state before ending the work session:

```bash
git status --short --branch
git stash list
bd ready
commit_sha=${commit_sha:-$(git rev-parse HEAD)}
git pull --rebase
post_pull_sha=$(git rev-parse HEAD)
test "$post_pull_sha" = "$commit_sha" || {
  echo "HEAD changed after evidence comment; add a correction Beads comment with $post_pull_sha before pushing"
  exit 1
}
bd dolt push
git push
git status
```

Before `bd dolt push`, file follow-up issues for any remaining work discovered during validation:

```bash
bd create --title="<short follow-up>" --description="<why this remains and what needs doing>" --type=task --priority=2
```

If temporary stashes or local-only branches were created, clear or explicitly hand them off before the final status check. Prune remote branches only when they are known to be stale and unrelated to active work; do not delete branches speculatively.

Final `git status` must say:

```text
Your branch is up to date with 'origin/main'.
nothing to commit, working tree clean
```

If `bd dolt push` or `git push` fails, resolve and retry before handoff.

## Expected Downstream State

After `4s9.31` closes:

- `4s9.32` should become ready or at least no longer be blocked by nanokernel preservation.

After `4s9.33` closes:

- `4s9.34` should become ready.
- `4s9.35` will still wait for `4s9.32` and `4s9.34`.

The final M9 acceptance suite remains out of scope for this plan.
