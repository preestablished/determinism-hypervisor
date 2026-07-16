# Suggestions (non-blocking)

## S1 — No guard keeping the 10 gated `pub mod` lines in sync (Angle 7)

`crates/dh-vmm/src/lib.rs:11-30` repeats `#[cfg(target_arch = "x86_64")]` over 10 separate
`pub mod` declarations. The header comment explains the split, but nothing enforces it: a
contributor who adds a new KVM-touching module later and forgets the attribute gets a *silent*
arm-lane breakage (or worse, an arm build that links x86-only symbols). Consider collapsing the
gated set into one attribute-bearing block so the gate is impossible to miss:

```rust
#[cfg(target_arch = "x86_64")]
mod x86 {
    pub mod boot; pub mod boundary; pub mod cpuid; pub mod hash; pub mod inject;
    pub mod kvm;  pub mod msr;      pub mod run;   pub mod runctl; pub mod tsc;
}
#[cfg(target_arch = "x86_64")]
pub use x86::*;
```

(or `cfg_if!`). One gate to maintain instead of ten. Same pattern applies to
`tools/dh-cli/src/lib.rs:7-22` (5 gated modules). Purely maintainability; current code is correct.

## S2 — Ungated kvm dev-deps in determinism-tests Cargo.toml

`tests/determinism/Cargo.toml:14-19` keeps `kvm-ioctls`, `nanokernel`, `vm-memory`, `libc` in a
plain `[dev-dependencies]` block. They are arm-buildable (cross-check passes), so this is not a
break — but they ARE compiled on the arm lane even though every test target that uses them gates
to empty. For consistency with the dh-vmm/dh-cli treatment and to shave arm build time, consider
moving them under `[target.'cfg(target_arch = "x86_64")'.dev-dependencies]`. Optional.

## S3 — `dh-cli caps` is reachable as a portable summary but blocked on arm

`crates/dh-vmm/src/lib.rs:81` `m0_missing_caps_summary()` is ungated and arm-buildable, yet the
arm `main.rs` stub (`tools/dh-cli/src/main.rs`) exits 2 with "requires an x86_64 host" before any
subcommand dispatch — so even the host-only `caps`/`cpuid` summary can't run on arm. This is a
defensible product decision (the debug CLI is an x86 tool), but if a Spark-side dev ever wants
`dh-cli caps` for the summary line, the all-or-nothing stub forecloses it. Worth a one-line
comment in the stub noting the intentional scope, or routing `caps` through even on arm. Minor.

## S4 — CI: arm leg now compiles materially more; watch wall-clock/cache

`.github/workflows/ci.yaml:38-39` — the arm lane went from `--exclude dh-vmm dh-worker dh-cli
determinism-tests` to plain `--workspace`, so it now builds dh-vmm (4 portable mods), dh-cli,
dh-worker, determinism-tests AND runs nanokernel's build.rs (nasm cross-assemble + rust-lld link).
On the shared self-hosted box this is more build/link work per run. The `concurrency:
cancel-in-progress` block (lines 14-16) mitigates queueing. No action needed now; just flag for
the next "CI is slow" investigation that the arm leg's footprint grew this iteration.
