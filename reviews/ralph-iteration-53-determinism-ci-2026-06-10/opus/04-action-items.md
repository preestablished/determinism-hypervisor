# Action items — iteration 53 determinism-ci

Verdict: **APPROVE.** No Criticals. Items below are hardening/nuance;
none block merge.

### Critical

_None._

### Important

- [ ] **(I1) Confirm the automation push token maps to an admin GitHub
  identity, not `infra-admin`.** Verified live: `infra-admin` has only
  `read`; the real pusher is `mattsp1290` (admin), so today the Ralph
  direct-merge flow works. If the push credential is ever rescoped to
  `infra-admin`, pushes to protected `main` will fail outright. No code
  change — just an ops confirmation / note in the runbook.

- [ ] **(I2) Add a one-line comment** at `got="$(live_value "$key")"` in
  `ci/check-determinism-class.sh` stating that a missing live field
  intentionally yields empty-string → drift (fail-closed), so a future
  refactor doesn't turn a missing field into an unclean `set -e` abort.

- [ ] **(I3) Add a `.gitattributes`** pinning the lock to LF
  (`ci/determinism-class.lock text eol=lf`, or `* text=auto eol=lf`).
  A CRLF lock falsely reds every key in the nightly. Verified live.

### Suggestions

- [ ] (S1) In `live_value`'s `*)` branch / the compare loop, emit a
  distinct "unknown lock key — fix the lock" error instead of folding a
  config typo into the "host drifted, re-baseline" message.
- [ ] (S2) Optional alternative to I3: strip trailing `\r` in-script
  (`line="${line%$'\r'}"`). Pick S2 or the `.gitattributes`, not both.
- [ ] (S3) Add an `if: failure()` notification step to
  `nightly-drift.yaml` (webhook or auto-filed issue) so a red tripwire
  isn't silent until someone opens the Actions tab.
- [ ] (S4) Add a `concurrency:` block to `nightly-drift.yaml` to document
  the single-runner serialization intent (currently relies on the
  one-job-per-runner default).
- [ ] (S5) **Do NOT** add `landing_precision` to the nightly canary — no
  drift mode it uniquely catches; keep the nightly lean (rationale in 02).
- [ ] (S6) One-line comment at `cpuinfo_field` that `grep -m1` reads CPU0
  only — fine for single-socket, revisit at any multi-socket re-baseline.

### Note (no action)

- `nightly-drift.yaml` is not yet on `main` (only `ci.yaml` is registered
  on GitHub). Scheduled triggers fire only from the default branch, so the
  nightly starts running after this branch merges — expected, not a defect.
