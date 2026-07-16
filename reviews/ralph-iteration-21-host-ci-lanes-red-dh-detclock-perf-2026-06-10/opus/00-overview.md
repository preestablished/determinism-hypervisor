# Review Overview

- **Branch:** `ralph/iteration-21-host-ci-lanes-red-dh-detclock-perf` vs `main`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Beads issue:** determinism-hypervisor-dx0
- **Scope:** CI-fix diff — make hosted CI lanes green after iteration 20's CI split

## Summary

Two surgical fixes for red hosted CI lanes:

1. **`crates/dh-detclock/src/counter.rs`** — `tests::pmu_available()` no longer
   trusts mere *existence* of `/proc/sys/kernel/perf_event_paranoid`. It now reads
   and parses the level and gates on `level <= 1 || euid == 0`. GitHub-hosted
   runners ship the sysctl at `paranoid=4`, which denies every unprivileged
   `perf_event_open` (EACCES) — previously the two PMU tests
   (`opens_pinned_guest_only_counter`, `reset_rezeroes`) ran and failed there.
   They now self-skip on hosted runners while still asserting the counter grant on
   the §7.4 lab box (`paranoid=1`) and the self-hosted `kvm-intel` lane.

2. **`.github/workflows/ci.yaml`** — the fmt gate was `cargo fmt --all -- --check`,
   whose `--all` also reformats the sibling path-dependency checkouts
   (`control-plane`, `guest-sdk`). Their formatting must not gate this repo. The
   step now scopes to workspace members via
   `cargo metadata --no-deps ... | jq ... | sed 's/^/--package /'`.

## Verification performed by this review

- **Threshold semantics (the key question):** confirmed `<= 1` is the *correct*
  threshold for this exact attr — see 01 for the full kernel-level analysis. Level
  2 would **not** suffice because the attr leaves `exclude_kernel = 0`.
- **Tests run on this box** (`paranoid=1`, euid=1000, non-root): both PMU tests
  pass — `cargo test -p dh-detclock --lib` → `2 passed; 0 failed`.
- **Repo convention:** `crates/dh-detclock/src/lib.rs` opens with
  `#![deny(unsafe_code)]`; the new `#[allow(unsafe_code)]` + `// SAFETY:` block
  matches the 8 existing targeted allows in this very file. `libc` is already a
  regular dependency, so `libc::geteuid()` needs no manifest change.
- **fmt pipeline:** expands to the 9 workspace members; `cargo fmt --check $(...)`
  exits 0. `jq` present on the runner image. YAML parses (`yaml.safe_load`).

## Verdict

**APPROVE**

The change is correct, minimal, well-commented, and the threshold logic is
precisely matched to the perf attr rather than copied from a generic recipe. The
findings below are one Important robustness note on the shell pipeline (silent
partial-scope on a jq/metadata failure) and minor suggestions — none block merge.

## Stats

| Metric | Value |
|---|---|
| Files changed | 2 |
| Lines added | ~16 |
| Lines removed | ~4 |
| Critical findings | 0 |
| Important findings | 1 |
| Suggestions | 4 |
| Tests run by reviewer | 2 (pass) |
