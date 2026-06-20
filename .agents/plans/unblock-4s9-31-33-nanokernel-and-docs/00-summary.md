# Unblock 4s9.31 And 4s9.33

Plan name: `unblock-4s9-31-33-nanokernel-and-docs`

Selected beads:

- `determinism-hypervisor-4s9.31` - Preserve nanokernel gates and golden fixtures after M9.
- `determinism-hypervisor-4s9.33` - Document Linux gate commands runner requirements and CI nightly classification.

## Why This Plan Exists

After `4s9.29` landed, `bd blocked` still reports downstream M9 work as blocked. The two next useful beads are stale-blocked:

- `4s9.31` lists only closed dependencies: `4s9.24`, `4s9.26`, `4s9.28`, `4s9.29`, and `4s9.7`.
- `4s9.33` lists only closed dependencies: `4s9.22`, `4s9.24`, and `4s9.29`.

The next coding agent should explicitly unblock and claim these two beads, run or collect the required evidence, update the docs/workflows if the current state is incomplete, then close them with evidence. Do not jump directly to `4s9.32`, `4s9.34`, or `4s9.35`; those are downstream consumers.

## Desired End State

`4s9.31` is closed with fresh evidence that the M9 Linux path did not weaken nanokernel coverage:

- `cargo test --workspace` passes.
- `cargo run -p dh-cli -- gate --runs 100` passes and still defaults to nanokernel.
- Current Phase 1 determinism tests pass.
- Nanokernel M5 record/replay corpus reverify passes.
- M7 nanokernel operator commands remain documented and valid.
- Existing nanokernel fixtures and corpus bytes are unchanged unless a dedicated follow-up bead authorizes a fixture change.

`4s9.33` is closed with updated operational docs and workflow classification:

- `docs/ops/test-partitioning.md` lists exact Linux gate commands, env vars, run counts, and CI/nightly/operator classification.
- `docs/ops/github-runner.md` records runner requirements for M9 Linux artifacts, slot affinity, pinned tools, and nightly canaries.
- `.github/workflows/ci.yaml` and `.github/workflows/nightly-drift.yaml` match the documented classification.
- Existing nanokernel CI/nightly lanes are preserved.

## File Map

- `01-current-state.md` records what is known now and what must be verified before editing.
- `02-beads-unblock-and-claim.md` gives the exact Beads workflow.
- `03-4s9-31-nanokernel-preservation.md` is the implementation and evidence runbook for `4s9.31`.
- `04-4s9-33-gate-docs-classification.md` is the implementation and evidence runbook for `4s9.33`.
- `05-validation-and-evidence.md` lists quality gates and evidence text to capture.
- `06-closeout-and-handoff.md` covers Beads comments, closure, git commit, push, and downstream state.
