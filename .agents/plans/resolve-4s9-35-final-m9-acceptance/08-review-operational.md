# Operational Review

Reviewer: subagent `Laplace`

The reviewer did not edit files.

## Findings

Blocking: the draft did not actually reserve the self-hosted runner. It only
performed point-in-time `pgrep` and `gh run list` checks before the suite.
A new `kvm-intel` CI/nightly job could start mid-run if the suite ran from an
operator shell, invalidating quiet-host M7 evidence. The runner docs make
single-runner serialization a GitHub-runner property, not a local-shell lock.

High: the slot-core preflight checked `taskset -c 2-5` in a child process, but
the nanokernel M7 commands were not wrapped in `taskset`. The harness checks
the test process affinity for cores `2-5`, so the draft could pass preflight
and then fail or produce misleading commands from a restricted shell.

Medium: nanokernel M7 preservation could inherit `DH_M7_ACCEPT_GUEST=linux`
from previous Linux commands and accidentally run the Linux path.

Medium: filtered ignored tests needed explicit nonzero-test checks. A stale
filter can exit successfully with `0 tests`.

Low: evidence should record both the tested code SHA and final docs/evidence
SHA. Final evidence should also record the actual image-cache keys.

## Requested Edits

- Add a hard runner reservation path: use one manually dispatched
  `kvm-intel` job for the suite, or stop/disable the repository runner
  service during an operator-shell run and restart it after.
- Add `grep Cpus_allowed_list /proc/self/status` for the current shell and
  wrap M7 commands with `taskset -c 2-5`.
- Prefix nanokernel M7 commands with `env -u DH_M7_ACCEPT_GUEST`.
- Reject filtered ignored-test transcripts with `0 tests`.
- Record both SHAs and image-cache keys.

## Resolution

All requested edits were applied in `02-reference-host-preflight.md`,
`03-acceptance-runbook.md`, `04-evidence-and-doc-updates.md`,
`05-failure-handling.md`, and `06-beads-and-closeout.md`.
