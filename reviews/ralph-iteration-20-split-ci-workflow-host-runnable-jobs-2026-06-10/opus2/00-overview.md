# CI Split + Clippy-Fix Review — Overview

- **Branch:** `ralph/iteration-20-split-ci-workflow-host-runnable-jobs` vs `main`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus (2nd reviewer)
- **Beads:** determinism-hypervisor-4jq

## Summary

This change splits `.github/workflows/ci.yaml` into a host lane (fmt + clippy `-D warnings` + build + test on an `[ubuntu-latest, ubuntu-24.04-arm]` matrix) and a `kvm-intel` self-hosted lane (build + test, fork-PR–guarded, with an explicit `/dev/kvm` probe), adds the missing `../guest-sdk` sibling checkout to both jobs (which fixes today's red `main`), fixes four pre-existing clippy warnings in `dh-vmm`, and applies `cargo fmt`. The fork-PR guard, the `/dev/kvm` probe, the guest-sdk checkout, and all four clippy fixes are correct and the clippy fixes are semantics-preserving (verified: `n % k != 0` ⇔ `!n.is_multiple_of(k)`, RNG call order unchanged; removed `MsrExitReason`/`VcpuExit` imports are genuinely unused). The one real problem is the **new `ubuntu-24.04-arm` matrix leg**: `crates/dh-vmm` imports x86_64-only `kvm_bindings` symbols (`kvm_msr_filter`, `kvm_msr_filter_range`, `KVM_MSR_FILTER_*`) **unconditionally with zero `cfg(target_arch)` gates**, and those symbols do **not** exist in `kvm-bindings` `arm64` bindings — so `cargo build --workspace` on the arm runner will fail to compile. The "locally verified all green" claim was x86_64-only; the new arm leg is untested and will be red.

## Verdict

**REQUEST_CHANGES**

The arm leg as written turns CI red on every run. Either gate `dh-vmm`'s x86 code paths behind `cfg(target_arch = "x86_64")` so the workspace builds on aarch64, or scope the arm matrix entry to the crates that are actually arch-portable (or drop it until the portability work lands). Everything else in the diff is good to merge.

## Stats

| Metric | Value |
|---|---|
| Files changed | 4 |
| Insertions / deletions | +56 / −11 |
| Critical issues | 1 |
| Important issues | 1 |
| Suggestions | 4 |
| Clippy fixes verified semantics-preserving | 4 / 4 |

## What I verified directly

- `kvm-bindings 0.14.0` `src/arm64/bindings.rs` contains **0** occurrences of `kvm_msr_filter_range` / `KVM_MSR_FILTER_DEFAULT_DENY` / `KVM_MSR_FILTER_READ`; all live only in `src/x86_64/bindings.rs`. `lib.rs` re-exports `arm64::*` (not `x86_64::*`) under `cfg(aarch64)`.
- `crates/dh-vmm/src/msr.rs:18-20` imports those symbols from `kvm_bindings` with no arch cfg; `dh-vmm` is a workspace member, so `cargo build --workspace` always builds it.
- `rng.next()` returns `u64`; `is_multiple_of` is stable for unsigned ints since Rust 1.87 and CI uses `dtolnay/rust-toolchain@stable` (1.93 locally) — fine.
- Sibling checkout paths (`repo`, `control-plane`, `guest-sdk`) match the `../control-plane` / `../guest-sdk` path deps in the root `Cargo.toml`.
- The fork-PR `if:` guard short-circuits correctly for push / same-repo PR / fork PR / re-run.
