# Review Overview — iteration-56 runner-finishing

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-56-runner-finishing`
- **Bead:** determinism-hypervisor-6eb (Register self-hosted GitHub runner labeled `kvm-intel`)
- **Verdict:** **APPROVE**

## Scope

Two-file ops change closing out the runner-registration bead:

1. `.github/workflows/nightly-drift.yaml` — adds a `concurrency` block
   (`group: kvm-intel-nightly-drift`, `cancel-in-progress: false`).
2. `docs/ops/github-runner.md` — rewrites the "One KVM job at a time" caveat to
   record the as-built concurrency split (ci.yaml = per-ref group + cancel
   true; nightly = static group + cancel false).

## Verification performed

| Check | Result |
|---|---|
| YAML parse (`yaml.safe_load`) | PASS — `on:` correctly parses as `True` key (YAML 1.1), concurrency block well-formed |
| Group-name collision with ci.yaml | NONE — ci group resolves to `ci-<ref>`; nightly is static `kvm-intel-nightly-drift` |
| Static group across cron + dispatch | Correct — both triggers share one group; with `false` a dispatch during a cron run QUEUES rather than cancels |
| Doc vs ci.yaml (per-ref group, cancel true, stale-run rationale) | MATCHES verbatim-level |
| "CI runs are stateless" claim | TRUE — ci.yaml kvm-intel job has no artifact/cache/baseline/upload steps |
| Runner service | `active` + `enabled` |
| Runner GitHub API | `online`, `busy:false`, labels `[self-hosted, Linux, X64, kvm-intel]` |
| Security policy (live) | token=`read`, `can_approve=false` — matches doc |
| Full workspace test suite | PASS — all `ok`, exit 0; live-KVM legs ran (46s/109s/135s), confirming /dev/kvm rw for the run-as user |
| `git status` after | clean |

## Bottom line

The change is correct, the doc is accurate and internally consistent, and bead
6eb's deliverables (runner user /dev/kvm + perf access, service install/restart,
slot-core isolation) are all documented and live-verified. Two non-blocking
observations are recorded in 01 and 02. Safe to merge PR #1.
