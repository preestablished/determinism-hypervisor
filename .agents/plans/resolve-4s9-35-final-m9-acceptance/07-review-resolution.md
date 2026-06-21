# Review Resolution

Two subagents reviewed this plan:

- Operational/KVM reference-host review: `08-review-operational.md`
- Evidence and Beads closeout review: `09-review-evidence.md`

## Outcome

The evidence/closeout reviewer found no blocking issue. The operational
reviewer found one blocking issue: the draft did not reserve the self-hosted
runner, so a CI/nightly job could start mid-suite and invalidate quiet-host M7
evidence.

## Edits Made

- Added a hard runner reservation section in `02-reference-host-preflight.md`.
  The implementation agent must either run the suite inside one reserved
  `kvm-intel` workflow job or stop the repository runner service during an
  operator-shell run and restart it during closeout.
- Added current-shell and `taskset -c 2-5` CPU-affinity checks.
- Wrapped nanokernel/default M7 acceptance commands in `taskset -c 2-5` and
  used `env -u DH_M7_ACCEPT_GUEST` so they cannot inherit Linux guest mode.
- Required every filtered or ignored test transcript to show named tests and
  reject `0 tests` evidence.
- Split `tested_code_sha` from `final_evidence_sha` in the evidence template.
- Required recording `DH_M9_IMAGE_CACHE` artifact keys in final evidence.
- Added explicit plan-directory commit guidance so this handoff can be pushed
  cleanly.
- Duplicated the `bd comment ... --stdin` evidence step in closeout before
  `bd close determinism-hypervisor-4s9.35`.
- Added a parent epic comment before closing `determinism-hypervisor-4s9`.

## Remaining Reviewer Guidance

The implementation agent should treat the runner reservation as mandatory for
operator-shell final evidence. A best-effort `gh run list` check is not enough
unless the whole suite is itself running under the single `kvm-intel` runner.
