# Suggestions (non-blocking)

## S1 — Stale `#![deny(unsafe_code)]` comment in dh-vmm (pre-existing, not made worse)

`crates/dh-vmm/src/lib.rs:1`

```
#![deny(unsafe_code)] // targeted allows in kvm.rs only (memfd_create, madvise, set_user_memory_region)
```

The comment says allows live "in kvm.rs only", but `#[allow(unsafe_code)]` actually appears across 7 modules: kvm.rs, inject.rs, tsc.rs, boundary.rs, runctl.rs, msr.rs, run.rs. This is **pre-existing** (identical on `main`) and this diff does not touch the comment or any of those allows, so it is not a regression. Per the review scope it is flagged, not fixed. If a future iteration touches lib.rs's header, consider rewording to "targeted allows in the x86_64 KVM modules" — note that all 7 modules are now x86-gated, which actually makes a generalized phrasing more natural.

## S2 — aarch64 local link path is a stub, not the CI path (informational)

The lab box has no qemu-user and no configured aarch64 linker, so `cargo test --target aarch64 --no-run` reaches the link stage and fails (host `collect2` can't emit an aarch64 ELF). The *compilation* fully succeeds (all object files emitted), which is what proves the determinism math compiles for arm; the CI `ubuntu-24.04-arm` runner links and runs natively. No action needed — just don't be alarmed by a local cross-link failure; it does not reflect a defect. If desired for local confidence, a `qemu-user-static` + `binfmt` setup or a `.cargo/config.toml` aarch64 linker entry would let the tests actually run here.

## S3 — Comment wording consistency in test gates (cosmetic)

The five test files use a hyphen-dash ("compiles to empty on other arches") while lib.rs/main.rs comments use the same phrasing. Consistent and clear; no change needed. Mentioned only to confirm the inner `#![cfg(target_arch = "x86_64")]` attributes are correctly placed (after the `//!` doc comments, before any `mod`/`use` items — legal crate-level inner attributes) in all five files.
