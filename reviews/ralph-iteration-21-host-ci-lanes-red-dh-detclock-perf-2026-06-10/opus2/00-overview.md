# Code Review — Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-21-host-ci-lanes-red-dh-detclock-perf` vs `main`
- **Bead:** determinism-hypervisor-dx0
- **Scope:** CI-fix diff (2 files, ~25 lines): `crates/dh-detclock/src/counter.rs` test gate + `.github/workflows/ci.yaml` fmt step.

## Summary

Two narrowly-scoped CI fixes, both addressing the same class of problem: a check
that was *too coarse* and produced false reds (the perf test gate) or false
greens / wrong scope (the fmt step).

1. **`pmu_available()`** — was a bare `Path::exists()` on
   `/proc/sys/kernel/perf_event_paranoid`. Now reads the file, parses the level,
   and requires `level <= 1 || euid == 0`. This correctly skips on
   GitHub-hosted runners (file exists, level 4 → every unprivileged
   `perf_event_open` is EACCES) while keeping a real failure a real failure on
   the §7.4 lab box (paranoid=1).

2. **fmt step** — was `cargo fmt --all -- --check`. `cargo fmt --all` *also*
   formats local path-based dependencies, i.e. the sibling checkouts
   (`control-plane`, `guest-sdk`), whose formatting must not gate this repo.
   The replacement enumerates this workspace's members via
   `cargo metadata --no-deps | jq | sed | tr` and passes them as `--package`
   flags.

I independently verified the core technical claims on the lab box:

- `/proc/sys/kernel/perf_event_paranoid` = `1`, `euid` = `1000` → `pmu_available()`
  returns `true` here, matching "locally verified, tests pass."
- `cargo fmt --help` confirms `--all` "Format all packages, **and also their
  local path-based dependencies**" — the rationale for the fmt change is real.
- The attr is **per-process** (`pid=0, cpu=-1`) with `exclude_kernel=0`. Against
  the kernel's paranoid semantics, `<=1` is *exactly* the right threshold for
  this specific attr (see 01).

The most material concern is **not** in the changed counter logic — that is
well-reasoned — but a **latent maintainability trap** in the fmt pipeline: at a
virtual-manifest workspace root, an empty argument substitution makes
`cargo fmt --check` a **no-op that exits 0**. I reproduced this. jq is
preinstalled on GitHub-hosted images so it is currently dormant, but the gate has
no defense if that ever changes (or if `cargo metadata` errors). One Important
hardening item; no blocking defects.

## Verdict

**Approve with one Important hardening recommendation (non-blocking).**

The behavioral changes are correct and the gate thresholds are precisely
matched to the underlying syscall requirements. The fmt pipeline works today but
should be made fail-closed; this can land as a fast-follow rather than block the
merge.

## Stats

| Category   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 1     |
| Suggestions| 4     |
| Positive   | 4     |

- Files reviewed: 2 (full) + research doc + workspace manifest
- Lines changed: ~25
- Independent verifications run: paranoid level, euid, `cargo fmt --all` semantics, virtual-manifest no-op fmt reproduction, default GHA shell flags (pipefail OFF), jq presence.
