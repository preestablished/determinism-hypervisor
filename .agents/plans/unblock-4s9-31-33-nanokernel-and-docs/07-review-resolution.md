# Review Resolution

Two subagents reviewed this plan after initial creation.

## Reviewers

- `019ee65d-8833-7d00-8df2-4909b00325e1` (`Kant`): Beads workflow, unblock/claim/closeout, dependency graph, and AGENTS.md closeout requirements.
- `019ee65d-a29e-7092-8879-066d1645d871` (`Nietzsche`): acceptance completeness, command accuracy, doc/workflow scope, and implementation feasibility.

## Accepted Changes

- Added explicit owner/operator decision points before moving stale-blocked beads from `BLOCKED` to `open`.
- Made `4s9.31` fixture integrity checks fail-closed with `git diff --exit-code` and `git status --porcelain` checks.
- Tightened `4s9.31` scope so it may add only a dated nanokernel-preservation addendum and must not claim `4s9.32` Linux-plus-nanokernel exit-gate acceptance.
- Added the missing Linux READY and Linux worker API command coverage to the `4s9.33` runbook.
- Replaced vague M4/M5 Linux command guidance with exact current test filters for `m4_transparency`, `m5_frame_scheduling`, and `m5_net_loopback`.
- Required `4s9.33` to audit and cite producer bead evidence from `4s9.22` through `4s9.30`, not only its direct dependencies.
- Switched closeout evidence examples to `bd comment --stdin` with quoted heredocs so multiline evidence is not corrupted by shell quoting or variable expansion.
- Added a pre-evidence `git pull --rebase`, a post-pull HEAD consistency check, follow-up issue creation guidance, stash check, and final handoff cleanup notes.

## Preserved Guidance

- Keep Beads reads/writes serial because the embedded Dolt backend takes an exclusive lock.
- Preserve existing nanokernel lanes and keep full Linux acceptance/operator gates out of required CI unless explicitly reclassified.
- Do not start downstream beads `4s9.32`, `4s9.34`, or `4s9.35` from this plan unless the user redirects scope.
