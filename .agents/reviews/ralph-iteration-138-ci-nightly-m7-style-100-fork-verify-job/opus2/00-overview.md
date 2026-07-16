# Review Overview

- Branch: `ralph/iteration-138-ci-nightly-m7-style-100-fork-verify-job`
- Date: 2026-06-16
- Reviewer: Claude Opus (2nd reviewer)

This branch adds a scheduled 100-child M7 fork/VerifyReplay canary to `nightly-drift`, documents the runner/test-partitioning expectations, and updates the M5 loopback acceptance test so its manual pre-`NET_RX` quantum uses the same unhashed canonical-input boundary that replay uses. The workflow wiring is mostly sound: the new job is in `alert-on-failure.needs`, the `inputs.* || default` pattern matches the existing scheduled/manual workflow usage, and `2-5` is consistent with the documented four isolated slot cores. I found one important hardening gap: the nightly job should explicitly disable the M7 harness's local-smoke skip escape hatch so a persistent self-hosted runner environment cannot turn prerequisite failures into a green canary.

Overall verdict: `REQUEST_CHANGES`

## Stats

- Files changed: 4
- Lines added: 62
- Lines removed: 6
- Commits: 1
