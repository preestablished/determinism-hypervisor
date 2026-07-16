# Code Review — Overview

- **Branch:** `ralph/iteration-20-split-ci-workflow-host-runnable-jobs` vs `main`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Beads issue:** determinism-hypervisor-4jq

## Summary

This change splits the single `rust` CI job into two lanes: a `host` lane (fmt,
clippy `-D warnings`, build, test) on the hosted matrix `[ubuntu-latest,
ubuntu-24.04-arm]`, and a `kvm-intel` lane (build + test on the self-hosted
Intel KVM box) gated by an `if:` so fork PRs never reach the runner. Both lanes
now also check out the `guest-sdk` sibling alongside `control-plane`, which is
the actual fix that unblocks `main` CI (iteration 19 added `../guest-sdk` path
deps for `detguest-host`/`detguest-wire` without the matching checkout, turning
`main` red). It also clears the four pre-existing clippy warnings in `dh-vmm`
(unused test imports + closure-body parens in `src/msr.rs`, two `is_multiple_of`
conversions in `src/agenda.rs`) and applies a `cargo fmt` reflow to the
iteration-19 smoke test. I verified all sibling repos are PUBLIC (so cross-repo
checkout needs no PAT), the `if:` guard correctly admits same-repo pushes/PRs
while excluding forks, and the lint fixes preserve semantics exactly.

## Verdict

**APPROVE**

The self-hosted gate matches the recommended pattern, the lint fixes are
semantics-preserving, and the guest-sdk checkout is the correct unblock. The
only findings are non-blocking robustness/hygiene suggestions (duplicate
push+pull_request double-runs, no `permissions:` block, unpinned third-party
action consistent with repo convention).

## Stats

- Files changed: 4
- Workflow: `.github/workflows/ci.yaml` (1 job → 2 jobs, +arm matrix, +guest-sdk checkout, +clippy gate, +/dev/kvm check)
- Source lint fixes: `crates/dh-vmm/src/msr.rs`, `crates/dh-vmm/src/agenda.rs`
- Formatting only: `crates/dh-devices/tests/detguest_host_smoke.rs`
- Critical: 0
- Important: 0
- Suggestions: 4
