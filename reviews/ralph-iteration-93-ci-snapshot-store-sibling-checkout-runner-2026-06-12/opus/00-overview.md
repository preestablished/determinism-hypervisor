# Review Overview

- **Branch:** `ralph/iteration-93-ci-snapshot-store-sibling-checkout-runner`
- **Base:** `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Commit:** 3a09a49 "ralph: iteration 93 checkpoint - runner tool provisioning doc (py3)"
- **Stats:** 1 file changed, +33 / -0, 1 commit

## Summary

This change adds a single "Tool provisioning (beyond the base Rust toolchain)"
section to `docs/ops/github-runner.md`, the runbook for the self-hosted
`kvm-intel` GitHub Actions runner. The section documents which tools the
milestone jobs (M5–M7) need beyond stable Rust, how the runner's captured PATH
(`.path`) exposes user-local installs to jobs, a status/install table for five
tools (`protoc`, `grpcurl`, `cargo-fuzz`, Rust nightly, `stress-ng`), and three
operator notes covering grpcurl version-stamping, nightly drift handling, and
the one remaining sudo-gated `stress-ng` install. I verified every load-bearing
factual claim against the repo: `ci/determinism-class.lock` exists, both
`.github/workflows/{ci,nightly-drift}.yaml` exist, `dh-proto` vendors protoc via
`protoc-bin-vendored`, and the sibling-repo crate `snapstore-client`
(`../snapshot-store/crates/snapstore-client`) also vendors it via a `build.rs`
that sets `PROTOC` to the vendored binary — so the "protoc not needed" claim is
accurate and correctly sourced. The writing is precise, internally consistent
with the rest of the runbook (PATH/.path mechanics, determinism-class framing,
public-repo posture), and actionable. The only substantive concern is a security
consistency gap: the file's own §"Security: public repo + privileged runner"
establishes this is a public-repo runner where fork-PR code execution is the
canonical hazard, yet the new section documents writable user-local tool
locations and `@latest`/unpinned installs without cross-referencing that posture.

## Verdict

**APPROVE** — factually accurate, internally consistent, and immediately
actionable. The security cross-reference and supply-chain caveats below are
non-blocking improvements to a docs-only change, not defects.
