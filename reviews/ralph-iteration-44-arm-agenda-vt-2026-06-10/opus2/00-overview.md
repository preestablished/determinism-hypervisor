# Review Overview — iteration 44: aarch64 workspace build (arm agenda/vt coverage)

- **Branch:** `ralph/iteration-44-arm-agenda-vt`
- **Base:** `main`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** v5w

## What this change does

Makes the entire workspace cross-compile on aarch64 by `cfg(target_arch = "x86_64")`-gating
every KVM-touching surface, so the arm CI lane can drop its `--exclude` list and gain real
coverage of the pure determinism math (agenda scheduling, virtual-time rationals, machine
config, block fixtures).

- `crates/dh-vmm/src/lib.rs`: agenda/blkfile/config/vt stay portable; boot, boundary, cpuid,
  hash, inject, kvm, msr, run, runctl, tsc gated to x86_64.
- `crates/dh-vmm/Cargo.toml`, `tools/dh-cli/Cargo.toml`: kvm-bindings / kvm-ioctls / libc /
  vm-memory / vmm-sys-util and the nanokernel dev-dep moved under
  `[target.'cfg(target_arch = "x86_64")'.dependencies]`.
- `tools/dh-cli`: `main.rs` git-mv'd into lib module `cli.rs` (sed `dh_cli:: -> crate::`),
  thin arch-dispatching `main.rs`, `cli`/`boot`/`cpuid`/`gate`/`run`/`skid` lib modules gated.
- `crates/dh-worker/src/preflight.rs`: `kvm_checks()` split — x86 real check + non-x86 honest
  `ok=false` stub.
- 5 live-KVM integration test files (`if0_deferral`, `regression`, `timer_determinism`,
  `boot_hello`, `skid_gate`) get `#![cfg(target_arch = "x86_64")]`.
- `.github/workflows/ci.yaml`: arm leg `cargo-args` becomes plain `--workspace`; comment refreshed.

## Verification performed (RUN, not eyeballed)

- **Byte-for-byte rename audit:** `git show main:.../main.rs | diff - cli.rs` — only `fn main`->`pub fn main`,
  `dh_cli::`->`crate::`, header comment, and the removed `#![forbid(unsafe_code)]` differ. All
  user-facing strings (usage text, JSON keys, error prefixes) are byte-identical.
- **aarch64 cross-check:** `cargo check --workspace --all-targets --target aarch64-unknown-linux-gnu`
  and `cargo clippy ... -D warnings` — both **exit 0**, all crates incl. nanokernel + determinism-tests.
- **x86 live boot:** `dh-cli boot hello.elf` prints `HELLO`; `--json` emits
  `{"serial":"HELLO
","exits":7}` (valid JSON); `caps`/no-arg/usage paths all correct (exit 0/0/2).
- **x86 live tests:** `cargo test -p dh-cli --test boot_hello` — 5 passed.
- **Portable test count:** `cargo test -p dh-vmm --lib` — 73 tests; agenda/vt/config/blkfile
  subset (~26 named + property tests) is what the arm lane now gains.
- **fmt:** clean on all changed crates.

## Stats

- 13 files changed (+294 / -206), 1 rename (main.rs -> cli.rs).
- 0 Critical, 0 Important, 4 Suggestions.

## Verdict

**APPROVE**

The gating is complete and adversarially verified: the full workspace cross-compiles and
clippy-passes on aarch64, the rename preserved every output byte, and the live x86 path is
intact (boot + tests green). The arm lane gains meaningful determinism-math coverage exactly
as the bead intended. Suggestions are maintainability-only and non-blocking.

### Local cross-check caveat (note, not a finding)

`cargo check`/`clippy` compile-only succeed on aarch64. `cargo test --target aarch64 --no-run`
cannot LINK on this x86 box (no aarch64 system linker/libc — `rust-lld: incompatible with
elf64-x86-64`). The native `ubuntu-24.04-arm` runner links natively, and nanokernel's
build.rs already ran on the arm lane pre-change (it was never excluded), so the linker
fallback chain is proven on the real runner. CI does what local check cannot: actually link
and run the arm test binaries.
