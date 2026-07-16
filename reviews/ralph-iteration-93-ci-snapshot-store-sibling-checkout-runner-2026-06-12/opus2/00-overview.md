# Review Overview

- **Branch:** `ralph/iteration-93-ci-snapshot-store-sibling-checkout-runner`
- **Base:** `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Commit:** `3a09a49` — "ralph: iteration 93 checkpoint - runner tool provisioning doc (py3)"
- **Stats:** 1 file changed, +33 / -0, 1 commit

## Summary

This change adds a "Tool provisioning" section to `docs/ops/github-runner.md`, documenting
the non-Rust tools (`grpcurl`, `cargo-fuzz`, Rust nightly, `stress-ng`) the milestone CI
lanes need on the self-hosted `kvm-intel` runner, plus the PATH-inheritance mechanism
(`.path` captured at `config.sh` time) and a per-tool status table dated 2026-06-12. I
independently verified every checkable factual claim against the live runner box and the
repo: the `protoc` "not needed" entry holds (both `crates/dh-proto/build.rs` and the sibling
`../snapshot-store/crates/snapstore-client/build.rs` set `PROTOC` via `protoc-bin-vendored`,
matching `docs/decisions/proto-seam.md`); `ci/determinism-class.lock` exists and confirms
kernel/microcode as the determinism class; `grpcurl` on the box really prints
`dev build <no version set>` and the documented `go version -m … | grep '^\s*mod'` recovery
command really yields `v1.9.3`; `cargo-fuzz 0.13.2` and nightly are installed; `stress-ng`
is genuinely absent. The prose is accurate, internally consistent, and unusually well
fact-checked. The findings below are about *durability* of this accuracy over time
(unpinned `@latest` / `cargo install` and auto-updating nightly undercut the runbook's own
reproducibility ethos), a security gap (user-writable tool dirs on a privileged public-repo
runner — the doc's own threat model), and a rebuild-flow seam (the Registration section does
not point operators back here to re-provision these tools). None of these block the merge of
a docs-only checkpoint.

## Verdict

**APPROVE** — with two Important follow-ups (version pinning + tool-dir integrity note) and
a handful of suggestions. All claims verified accurate; issues are forward-looking, not
corrections.
