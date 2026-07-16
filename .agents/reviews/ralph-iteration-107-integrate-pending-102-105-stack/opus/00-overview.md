Branch: `ralph/iteration-107-integrate-pending-102-105-stack`
Base: `main`
Date: 2026-06-15
Reviewer: Codex Reviewer 1

The branch integrates the pending slot-manager, worker service shell, runtime-table foundation, and DHSNAP MCFG decode stack onto current `main`. The implementation is mostly conservative and keeps mutating RPCs from faking success, but the initial review found one important DestroyVm lifecycle ordering issue.

Verdict: REQUEST_CHANGES

Stats: 18 files changed, 3067 insertions, 38 deletions.
