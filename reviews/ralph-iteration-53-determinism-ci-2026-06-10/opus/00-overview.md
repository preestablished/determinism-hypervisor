# Review Overview — iteration 53, determinism-ci cluster (q10 + 8n7)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-53-determinism-ci`
- **Diff:** `git diff main...HEAD` — 3 new files (+151), plus a pre-existing
  tracked `ci/determinism-class.lock` (added iteration 3) and a live
  branch-protection change via `gh api`.
- **Verdict:** **APPROVE**

## Scope

CI/policy iteration, no engine code:

1. `ci/check-determinism-class.sh` (NEW) — host-tuple drift comparator
   (7 keys, byte-exact vs live `/proc/cpuinfo` + `uname`).
2. `.github/workflows/nightly-drift.yaml` (NEW) — cron 03:17 UTC + dispatch
   on `[self-hosted, kvm-intel]`; job 1 drift check, job 2 (`needs` job 1)
   the determinism canary (`regression` + `counting_semantics` +
   `counting_smoke`).
3. `CONTRIBUTING.md` (NEW) — merge policy.
4. Branch protection set live on `main`.

## Live verification performed

| Check | Result |
|---|---|
| Script green vs live host | exit 0, 7 keys ok |
| Script vs doctored lock (microcode 0xfa→0xff) | exit 1, drift reported |
| Script vs bogus key `bogus_key=x` | exit 1, clean drift (NOT an unclean `set -e` crash) |
| Script vs CRLF lock | exit 1, every key falsely drifts (fail-closed) |
| Script vs trailing-whitespace value | exit 1, drift (fail-closed) |
| shellcheck | clean (exit 0) |
| Canary set (`regression` 1e9-twice + counting) | all pass, exit 0 (regression 4.19s) |
| clippy `-p determinism-tests --all-targets -D warnings` | clean |
| YAML parse (both workflows) | OK |
| Protection contexts vs ACTUAL check-run names on main HEAD | **byte-exact match** |
| Pusher GitHub identity & permission | `mattsp1290` = **admin** (the real push identity) |

## Headline

No Criticals. The two highest-risk failure modes for a "required-check"
iteration — (a) protection context names not matching the real check-run
names (would make every PR permanently unmergeable) and (b) the merge
pusher lacking admin to bypass the gate — were both checked against the
live GitHub API and are **correct**. The script is fail-closed on every
adversarial input tried. Approving; the items below are nuance and
hardening, not blockers.
