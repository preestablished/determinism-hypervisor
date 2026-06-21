# Resolve 4s9.35 Final M9 Acceptance

Plan name: `resolve-4s9-35-final-m9-acceptance`

Selected bead: `determinism-hypervisor-4s9.35` - Run full M9 acceptance suite and publish final evidence.

## Why This Bead

This is the best blocker to fix next because this repository is currently on
the Linux/KVM reference host. The other blocked beads either require a human
decision or an upstream tree that is not reachable from this host. `4s9.35`
is different: its direct dependencies are closed, and the remaining work is
operator-run evidence collection on this exact class of machine.

`bd list --status blocked` still shows both `4s9.35` and its parent `4s9`
as blocked, but `bd show determinism-hypervisor-4s9.35` shows all direct
dependencies closed:

- `4s9.30` - Linux worker API integration tests.
- `4s9.32` - Phase 1 and Phase 2 exit-gate Linux/nanokernel evidence docs.
- `4s9.33` - Linux gate commands, runner requirements, and CI/nightly classification.
- `4s9.34` - accepted drift ledger.

The practical fix is to move `4s9.35` out of stale blocked status, run the
final acceptance suite with no `*_ALLOW_SKIP=1` evidence, publish final notes,
then close `4s9.35`. If all M9 children are closed after that, close the
parent epic `determinism-hypervisor-4s9`.

## Reference Host Assumption

This plan assumes the implementation agent is on `infra-control`, the
Linux/KVM reference machine documented by:

- `docs/ops/test-partitioning.md`
- `docs/ops/github-runner.md`
- `ci/determinism-class.lock`

The host matters. Do not replace final Linux evidence with skip-enabled,
non-KVM, generic CI, or laptop smoke evidence. The full final acceptance run
must use live `/dev/kvm`, the staged M9 reference-workload artifacts, and the
isolated slot-core set `2-5` where the M7 commands require it.

## Desired End State

The implementing agent leaves the repository in this state:

- `4s9.35` is closed with the exact final acceptance evidence.
- `4s9` is closed if `bd show determinism-hypervisor-4s9` confirms all child
  beads are closed.
- The final evidence is recorded in Beads and, where needed, in
  `docs/phase-1-exit-gate.md` and `docs/phase-2-exit-gate.md`.
- The final evidence includes artifact paths, artifact BLAKE3 hashes, host
  kernel and microcode, determinism-class status, command results, and any
  workflow run links used.
- `bd dolt push` and `git push` have both succeeded.
- `git status` reports the branch is up to date with origin and the working
  tree is clean.

## File Map

- `01-current-state.md` records the bead graph, authority docs, and stale-blocked state.
- `02-reference-host-preflight.md` defines the host, artifact, KVM, and scheduling checks.
- `03-acceptance-runbook.md` gives the exact command sequence for final evidence.
- `04-evidence-and-doc-updates.md` defines what to capture and where to publish it.
- `05-failure-handling.md` covers failure triage without weakening acceptance.
- `06-beads-and-closeout.md` gives Beads, commit, Dolt, and Git closeout steps.
- `07-review-resolution.md` summarizes subagent reviews and resulting edits.
- `08-review-operational.md` contains the KVM/reference-host review.
- `09-review-evidence.md` contains the evidence/bead-closeout review.

## Non-Goals

Do not redesign M9 Linux contracts, artifact format, command-line policy, or
gate classification while implementing this plan. Those decisions are already
recorded in the `4s9.30` through `4s9.34` dependency chain. If final
acceptance exposes a real defect, file or update a bead and fix the defect;
do not close `4s9.35` by changing the definition of acceptance.
