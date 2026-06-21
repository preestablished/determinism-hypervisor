# Beads And Closeout

## Start Of Work

Use sequential `bd` commands. Do not run Beads reads/writes in parallel with
the embedded Dolt backend.

```bash
bd show determinism-hypervisor-4s9.35
bd update determinism-hypervisor-4s9.35 --status open \
  --append-notes "Unblocking stale blocked state for final M9 acceptance run on the reference Linux/KVM host."
bd update determinism-hypervisor-4s9.35 --claim
```

If `bd update --claim` fails because someone else has claimed the bead, stop
and coordinate instead of stealing ownership.

## Commit Strategy

`4s9.35` is primarily evidence work. Expected source-controlled changes are
usually limited to:

- `docs/phase-1-exit-gate.md`
- `docs/phase-2-exit-gate.md`
- `.agents/plans/resolve-4s9-35-final-m9-acceptance/` while creating or
  revising this handoff plan.

There may be no code change unless the suite exposes a defect. If code changes
are needed, keep them in focused commits and rerun the affected gate plus the
full final acceptance suite.

Do not stage:

- raw logs under `target/`;
- M9 artifact bytes;
- image-cache entries;
- unrelated worktree changes.

Recommended final commit if only docs changed:

```bash
git add docs/phase-1-exit-gate.md docs/phase-2-exit-gate.md
git commit -m "Publish final M9 acceptance evidence"
```

If the plan directory is still untracked or modified during a planning-only
session, commit it intentionally:

```bash
git add .agents/plans/resolve-4s9-35-final-m9-acceptance
git commit -m "Plan final M9 acceptance closeout"
```

If no tracked files changed and all evidence is in Beads comments only, there
may be no Git commit. Still run `bd dolt push` and `git status`.

## Closing 4s9.35

After the final acceptance suite passes, post durable evidence before closing:

```bash
final_evidence_sha=$(git rev-parse HEAD)
bd comment determinism-hypervisor-4s9.35 --stdin <<EOF
<paste final evidence summary, including tested_code_sha and final_evidence_sha=$final_evidence_sha>
EOF
```

Then close:

```bash
bd close determinism-hypervisor-4s9.35 \
  --reason "Full M9 acceptance suite passed on the reference Linux/KVM host with no skip-enabled Linux evidence; final evidence published for workspace, nanokernel, Linux Phase 1, Linux M4/M5, Linux worker API, and Linux M7 gates."
```

Confirm the close:

```bash
bd show determinism-hypervisor-4s9.35
```

## Closing The Parent Epic

Only close the parent after confirming all children are closed:

```bash
bd show determinism-hypervisor-4s9
```

If the child list reports all complete:

```bash
bd comment determinism-hypervisor-4s9 --stdin <<'EOF'
All M9 child beads are closed. Final acceptance evidence is recorded on 4s9.35.
EOF
bd close determinism-hypervisor-4s9 \
  --reason "All M9 child beads are closed and final M9 acceptance evidence has been published."
```

If any child remains open, in progress, or blocked, do not close the epic.
Update that child or file the missing follow-up.

## Required Validation Before Push

For docs-only evidence updates:

```bash
git diff --check
rg -n "M9 final acceptance|DH_M9_ALLOW_SKIP=0|DH_M7_ACCEPT_ALLOW_SKIP=0|verified=1000|divergence=0" \
  docs/phase-1-exit-gate.md docs/phase-2-exit-gate.md
```

For code changes, also run:

```bash
cargo fmt --check
cargo test --workspace
```

Do not skip the full acceptance commands from `03-acceptance-runbook.md`
just because the docs validation passes.

## Mandatory Session Closeout

The repository instructions require pushing both Beads and Git state before
ending the session:

```bash
sudo -n systemctl start actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service || true
systemctl is-active actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service || true
git status --short --branch
git pull --rebase
bd dolt push
git push
git status
```

The final `git status` must say the branch is up to date with origin and the
working tree is clean.

If `git pull --rebase` changes code after evidence was gathered, rerun the
affected acceptance commands. If it only replays docs/evidence commits, record
both `tested_code_sha` and the final evidence/docs SHA and state that the delta
is docs-only. The cleanest path is to pull/rebase before the long final suite,
then avoid changing code until evidence is committed.

If `bd dolt push` or `git push` fails, resolve the failure and retry. Do not
hand off with local-only final evidence.
