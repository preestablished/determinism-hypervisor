# Review — ralph/iteration-53-determinism-ci (2nd reviewer)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-53-determinism-ci`
- **Scope:** `git diff main...HEAD` — `ci/check-determinism-class.sh`, `.github/workflows/nightly-drift.yaml`, `CONTRIBUTING.md`, plus the LIVE branch-protection state (read via `gh api`).
- **Beads:** q10 (drift comparator), 8n7 (branch protection / required check).

## Verdict: APPROVE (with documentation follow-ups)

The code that landed is correct and well-built. The comparator passes shellcheck
clean, exits 0 on the live host (this box, 7 keys), exits 1 on doctored drift,
and fails closed on a missing or zero-key lock. Both workflow YAMLs parse. The
canary's three test targets exist (`regression`, `counting_semantics`,
`counting_smoke` in package `determinism-tests`) and compile. The branch
protection is live and matches the stated intent (3 required contexts;
`enforce_admins=false`; no user restrictions; non-strict).

I am approving rather than blocking because nothing here is *wrong* — but two
policy/operational facts are load-bearing and **undocumented**, and one parse
edge in the comparator is latent. None block merge; all should be filed.

## The two verdicts asked for

- **Fork-PR verdict — INTENDED but UNDOCUMENTED (Important).** Repo is
  **public** (`gh api repos/... .private == false`). `kvm-intel` is a
  **required** status check. The `kvm-intel` job's `if:` guard
  (`push || head.repo.full_name == github.repository`) means a fork PR
  produces **no `kvm-intel` check run at all** → the required check never
  reports → **fork PRs are permanently unmergeable.** This is the correct
  security posture (never run untrusted fork code on the lab box), but on a
  *public* repo it is a silent "we do not accept external PRs" policy. It must
  be stated in CONTRIBUTING so a drive-by contributor isn't left with a PR that
  can never go green. Not a code bug — a documentation gap with reputational
  cost on a public repo.

- **Box-offline verdict — INTENDED posture, but no documented escape (Important).**
  If the kvm-intel runner is offline, the `kvm-intel` check never reports and
  **every same-repo PR is unmergeable** (and admin direct-push is the only way
  in — see the lockout matrix in 01). For a single-self-hosted-box product this
  is the honest fail-closed default, but CONTRIBUTING currently says nothing
  about what to do when the box is down. The intended answer (admin merges
  directly once the suite has been run, or the runner is brought back) should
  be written down.

See `01-critical-and-important.md` for the full lockout matrix, `02-suggestions.md`
for the comparator parse-robustness note and the nightly-failure-routing
proposal, and `04-action-items.md` for the filed list.
