# Action items — ralph/iteration-53-determinism-ci (2nd reviewer)

Verdict: **APPROVE.** Nothing here blocks merge. The items below are
documentation and robustness follow-ups; the strongest two are Important and
should be filed as beads before this is considered "done."

### Critical

_None._

### Important

- [ ] **AI1. Document the fork-PR policy in CONTRIBUTING.** Repo is public and
  `kvm-intel` is required, but fork PRs never trigger `kvm-intel` (the `if:`
  guard skips it → no check run → permanently unmergeable). Add an "External
  contributions" section stating: hardware-in-the-loop check is required, fork
  PRs can't run it, so external changes are landed by a maintainer from a
  same-repo branch. Self-contained fix: edit `CONTRIBUTING.md` only.

- [ ] **AI2. Document the "kvm-intel runner is offline" procedure.** When the
  box is down, every same-repo PR is unmergeable (required check never
  reports). Add one paragraph (CONTRIBUTING or `docs/ops/github-runner.md`):
  diagnose/restart the runner first; only an admin, only after running the live
  suite on the box, lands via direct push, recording why in the commit. Make
  the bypass deliberate and auditable.

- [ ] **AI3. Version the branch-protection state in-repo.** It exists only as
  mutable GitHub server state (any admin can silently remove it; no audit, no
  reviewable diff). Commit `ci/branch-protection.json` (the live
  `required_status_checks` contexts, `enforce_admins=false`, etc.) plus a
  re-apply command in the runbook:
  `gh api -X PUT repos/preestablished/determinism-hypervisor/branches/main/protection --input ci/branch-protection.json`.
  Makes the gate reconstructible and drift in the gate itself detectable.
  **This is the item most worth a human's eyes** (8n7 set protection via API;
  capturing it is what closes the loop for human review).

### Suggestions

- [ ] **AI4. (S1) Make the comparator terminator-tolerant.** Change the read
  loop to `while IFS= read -r line || [[ -n "$line" ]]; do`. Without it, a lock
  whose final line lacks a trailing newline silently drops that key from the
  check, and the zero-keys guard won't catch it. Latent today (committed lock
  ends in `\n`), but cheap insurance against a whitespace edit disarming the
  tripwire. One-line change in `ci/check-determinism-class.sh`.

- [ ] **AI5. (S3) Add nightly-failure routing.** A `notify` job gated on
  `if: failure()`, on a **hosted** runner (so a down kvm box doesn't also kill
  the alarm), with `permissions: issues: write`, using `actions/github-script`
  to open-or-update a pinned tracking issue. A silent-red determinism nightly is
  the worst outcome for this product. Implementable as a separate bead.

- [ ] **AI6. (S2) Assert an expected key-set/count in the comparator.** Turn
  "host_kernel silently missing" from green into red by validating the lock
  defines the known schema, not merely ">0 keys." More robust version of AI4.
  Optional.

- [ ] **AI7. (S4) Add a `concurrency` group to `nightly-drift.yaml`** matching
  ci.yaml's pattern, so a manual dispatch can't contend with the cron for the
  single box. Optional, cosmetic.

- [ ] **AI8. (S5) Add a one-line comment** in the comparator noting it's meant
  to run only on the kvm-intel runner and that the `cpu_*` keys double as a host
  fingerprint (so a wrong-host run fails closed). No behavior change. Cosmetic.

## Verification performed (this review)

- `shellcheck ci/check-determinism-class.sh` → clean (exit 0).
- Comparator live on this box → exit 0, 7 keys ("determinism class matches").
- Comparator vs doctored lock (drifted `host_kernel`, comment + blank +
  `=`-in-value lines) → exit 1, correct per-key drift report.
- Comparator vs all-comment lock → exit 1 (zero-keys guard).
- Comparator vs missing lock → exit 1 (lock-missing guard).
- Comparator vs no-trailing-newline lock → **dropped the last key** (S1/AI4).
- Committed lock `xxd`-confirmed to end in `\n` → S1 is latent, not active.
- `gh api repos/... .private` → `false` (public); branch protection read live
  (3 required contexts, `enforce_admins=false`, no restrictions, non-strict).
- Both workflow YAMLs `yaml.safe_load` OK.
- Canary targets exist + compile (`determinism-tests`: `regression`,
  `counting_semantics`, `counting_smoke`).
- Working tree clean after review (no files modified outside `reviews/`).
