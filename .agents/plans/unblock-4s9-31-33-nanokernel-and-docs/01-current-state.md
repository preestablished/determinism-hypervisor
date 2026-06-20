# Current State

## Beads

`bd blocked` currently reports:

- `4s9.32` blocked by `4s9.31`.
- `4s9.34` blocked by `4s9.33`.
- `4s9.35` blocked by `4s9.32`, `4s9.33`, and `4s9.34`.

The target beads are stale-blocked:

- `4s9.31` is `BLOCKED`, but all listed dependencies are closed.
- `4s9.33` is `BLOCKED`, but all listed dependencies are closed.

Use serial `bd` reads and writes. The embedded Dolt backend takes an exclusive lock, and parallel `bd show` calls can fail with:

```text
another process holds the exclusive lock on .beads/embeddeddolt
```

## Relevant Current Evidence

`4s9.29` closed with Linux M7 evidence on `infra-control`:

- commit `f507c5846312d1e225d70b84513221b97c5caa9f`;
- full Linux M7 acceptance: `verified=1000 divergence=0 unique_hashes=1 epoch_hashes=1000`;
- targeted Linux cross-slot: 10 sampled indices matched same-seed refs/logs across slots;
- Linux 100-child nightly canary added in `.github/workflows/nightly-drift.yaml`;
- `docs/ops/test-partitioning.md` gained Linux and nanokernel M7 rows.

Treat this as prior evidence, not a replacement for `4s9.31` and `4s9.33` acceptance. The next agent still needs to inspect current files and run the commands required by those two beads.

## Current Docs To Audit

Primary files:

- `docs/phase-1-exit-gate.md`
- `docs/phase-2-exit-gate.md`
- `docs/ops/test-partitioning.md`
- `docs/ops/github-runner.md`
- `.github/workflows/ci.yaml`
- `.github/workflows/nightly-drift.yaml`

Known starting points:

- `docs/phase-1-exit-gate.md` and `docs/phase-2-exit-gate.md` contain pre-M9 baseline refresh sections dated 2026-06-18. They likely need fresh post-M9 nanokernel preservation evidence for `4s9.31`.
- `docs/ops/test-partitioning.md` already lists M9 artifact env vars and Linux M7 commands. It still needs an audit against all Linux gate commands required by `4s9.33`, not just M7.
- `docs/ops/github-runner.md` describes M9 artifact staging and the original nanokernel M7 nightly. It should be checked for the new Linux M7 nightly and any M9 Linux artifact/slot-affinity requirements added after `4s9.29`.
- `.github/workflows/nightly-drift.yaml` has both nanokernel and Linux M7 nightly jobs after `4s9.29`.
- `.github/workflows/ci.yaml` intentionally runs non-ignored workspace gates and should not gain long operator-only Linux acceptance jobs unless the docs classify them as required.

## Constraints

- Do not update or regenerate files under `tests/nanokernel/**` for `4s9.31`.
- Do not update checked-in corpus fixtures unless a dedicated follow-up bead explicitly accepts that fixture change.
- Do not weaken nanokernel commands to skip-only evidence.
- Do not turn operator-run Linux gates into CI/nightly jobs unless the docs and runtime budget justify that classification.
- Preserve the existing nanokernel lanes when adding or documenting Linux lanes.
