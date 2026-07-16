# Critical & Important

## Critical

_None._ The change is correct as written. No data loss, no security regression
(the fork guard is the *right* call), no broken gate.

---

## Important

### I1. The lockout-scenario matrix — fork PRs are permanently unmergeable, and it's undocumented

Verified facts:
- Repo is **public**: `gh api repos/preestablished/determinism-hypervisor --jq .private` → `false`.
- `main` branch protection (live): required contexts =
  `["kvm-intel", "host (ubuntu-latest, --workspace)", "host (ubuntu-24.04-arm, --workspace)"]`,
  `strict=false`, `enforce_admins=false`, no user/team `restrictions`.
- `kvm-intel` job `if:` (ci.yaml) =
  `github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository`.

Concrete outcomes for each actor:

| Scenario | What happens | OK? |
|---|---|---|
| (a) Admin (ralph) direct merge-push to `main` | `enforce_admins=false` → push allowed; `push` event triggers ci (`host` legs) and triggers `kvm-intel` (`event_name=='push'` arm true). Required checks run *after* the push but don't block the push itself. | Intended (autonomous flow). |
| (b) Same-repo PR, green checks | All 3 contexts run and pass → mergeable. | Correct. |
| (c) Same-repo PR, **kvm box OFFLINE** | `host` legs pass, but `kvm-intel` check **never reports** (no runner to pick it up; it sits queued). Required check missing → **PR cannot merge**. Only path forward: admin direct-push, or bring the box back. | Intended fail-closed, but **undocumented**. |
| (d) **Fork PR** | `kvm-intel` `if:` is false → job is **skipped and produces NO check run** (a skipped-by-`if` job does not post the context). `host` legs run. Required `kvm-intel` context never appears → **PR is permanently unmergeable, even with green host legs.** | Intended security posture, but **silently undocumented on a public repo.** |

**Why this matters.** On a *public* repo, (d) means the project silently does
not accept external contributions through the normal PR flow — a forked
contributor will see two green checks and one forever-pending `kvm-intel` with
no explanation. That's a poor contributor experience and, more importantly, an
*undocumented policy*. The security choice itself is correct (never execute
untrusted code on a runner with `/dev/kvm` and full lab access). The fix is
documentation, not code.

**Recommendation (documentation):** Add a short "External contributions"
section to `CONTRIBUTING.md` stating plainly: this repo runs a required
hardware-in-the-loop check on a self-hosted box; fork PRs cannot trigger it and
therefore cannot merge; external contributors should open an issue / discuss,
and a maintainer will land the change from a same-repo branch (where the fork
guard's `head.repo.full_name == github.repository` arm becomes true). This
turns a silent dead-end into a stated workflow.

### I2. CONTRIBUTING is silent on "the box is down"

For a product whose entire merge gate is one self-hosted runner, the
single-point-of-failure operational answer must be written down. Today
CONTRIBUTING says the check is required and never worked around — good — but
not what the on-call/maintainer does when the runner is genuinely offline
(hardware, network, GitHub runner-agent crash). Without it, the implicit answer
is "admin force-merges," which quietly erodes the very gate this iteration
built.

**Recommendation:** One paragraph in CONTRIBUTING (or the runbook) under "When
the kvm-intel runner is unavailable": diagnose/restart the runner agent first
(point at `docs/ops/github-runner.md`); only an admin, only after running the
live suite locally on the box, lands the PR via direct push, and records why in
the commit. Make the bypass deliberate and auditable rather than ambient.

### I3. Branch-protection state is not versioned anywhere in the repo

GitHub does not version branch-protection config. The live protection
(verified above) exists only as mutable server state that **any admin can
silently remove or weaken**, with no audit trail in the repo and no
PR-reviewable diff. For a product where "the gate is the floor for everyone,"
the gate's own definition should be reconstructible from the repo.

**Recommendation (low effort, high leverage):** Commit
`ci/branch-protection.json` capturing the intended `required_status_checks`
contexts, `enforce_admins`, etc., plus a one-line re-apply command in the
runbook, e.g.:

```bash
gh api -X PUT repos/preestablished/determinism-hypervisor/branches/main/protection \
  --input ci/branch-protection.json
```

This makes drift in the protection itself detectable (the file is the
source-of-truth; a human can diff live vs file) and makes the gate
re-applicable after an accidental deletion. Judge: Important for a
determinism/safety product, since the protection is the only thing standing
between "reviewed" and "merged."
