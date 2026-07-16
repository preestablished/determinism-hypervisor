# Workflow

## Finding: matrix doubles the 24h operator dispatch on `kvm-intel`

Severity: High

References:

- `.github/workflows/nightly-drift.yaml:12`
- `.github/workflows/nightly-drift.yaml:16`
- `.github/workflows/nightly-drift.yaml:19`
- `.github/workflows/nightly-drift.yaml:117`
- `.github/workflows/nightly-drift.yaml:120`
- `.github/workflows/nightly-drift.yaml:150`

The existing workflow documentation says the operator command:

`gh workflow run nightly-drift.yaml -f fuzz_seconds=86400 -f fuzz_runner=kvm-intel`

costs one 24h dispatch and delays the scheduled drift/canary run by up to about 24h. The branch changes `dhilog-fuzz` to a two-target matrix, and each matrix leg receives the full `fuzz_seconds` value.

On hosted runners, the two matrix legs can run in parallel. On the documented 24h accept path, the runner is the single `kvm-intel` box, so the `dhilog_parse` and `dhilog_splice` legs serialize. A manual `fuzz_seconds=86400` dispatch therefore becomes up to roughly 48h of fuzzing while the workflow concurrency group remains occupied. The comments and input descriptions still promise the old 24h behavior.

Suggested fix:

Preserve the operator contract explicitly. Options include:

- keep one `dhilog-fuzz` job for operator dispatch and divide the requested duration across both targets in shell;
- add an explicit dispatch input selecting one target for 24h acceptance runs while scheduled runs cover all targets;
- remove the matrix and run both targets from one step with a computed per-target duration so `fuzz_seconds` remains a total budget.

If the intent is actually 24h per target, update the dispatch documentation, timeout reasoning, and operator warning to say the kvm-intel run can occupy the workflow and runner for about 48h.

## Non-finding: per-target corpus isolation

The matrix-specific cache paths and artifact names are good. They prevent parse and splice corpora from clobbering each other and make crash artifact lookup unambiguous.
