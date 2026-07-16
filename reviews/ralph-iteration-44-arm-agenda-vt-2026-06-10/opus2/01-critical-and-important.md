# Critical and Important Findings

## Critical

**None.**

Adversarial cross-compile (`cargo check`/`clippy --workspace --all-targets --target
aarch64-unknown-linux-gnu`) both exit 0. No gated-module reference leaks from ungated code:
every `dh_vmm::{kvm,boot,run,msr,inject,tsc,cpuid,hash,boundary,runctl}` use sits inside a
gated lib module (dh-cli's boot/run/gate/skid/cpuid/cli) or the gated `kvm_checks()` in
dh-worker. The bead's named highest-value risk — a gated dev-dep used by an ungated test —
does NOT occur: the portable modules' test blocks (agenda/vt/config/blkfile) contain zero
nanokernel/kvm references (grepped + cross-compiled clean), and `nanokernel` is correctly
under `[target.'cfg(target_arch="x86_64")'.dev-dependencies]` (dh-vmm) and the gated deps
block (dh-cli, its only non-test user is the gated skid.rs).

## Important

**None.**

Specifics checked and cleared:

- **Rename did not corrupt output (Angle 1):** `git show main:tools/dh-cli/src/main.rs | diff - cli.rs`
  shows the sed `s/dh_cli::/crate::/` touched only 6 code-path references. The `usage()` help
  text, all JSON output literals (`"reason"`, `"icount"`, `"serial"`, `"exits"`, etc.), and all
  `dh-cli <subcmd>:` error prefixes are byte-identical. Verified live: `boot --json` emits valid
  JSON, `caps` and usage paths match exactly. No stdout regression for consuming scripts/CI.

- **dh-worker preflight (Angle 5):** `run_preflight()` (ungated) calls `kvm_checks()`, which
  exists on both arches (real + stub) — resolves on arm. No CI step runs `dh-workerd
  --preflight`, so the always-`ok=false` arm variant cannot turn the lane red. The stub's
  message (`got: target_arch=aarch64`, `want: x86_64 (VMX)`) is honest and correct.

- **config.rs public API (Angle 4):** ungated and clean — `CpuidLeaf` is a plain local struct
  (not a kvm_bindings re-export), `BootSpec`/`MachineConfig` carry no kvm types; only dep is
  `crate::vt::ClockRatio`. Safe to keep ungated.

- **determinism-tests common/mod.rs (Angle 6):** it is a `tests/common/mod.rs` subdir module,
  never compiled standalone; both includers (`if0_deferral`, `timer_determinism`) are now
  `#![cfg]`-gated, so it only builds inside x86 targets. Cross-check confirms it compiles to
  nothing on arm.
