# Action Items

Reviewer: Claude Opus (2nd reviewer) · 2026-06-10 · branch `ralph/iteration-36-determinism-regression` (bead p9g)

### Critical

_None._ The 1e9 determinism gate passed 5/5 sequential timed runs, passed serial (`--test-threads=1`) and parallel, and the full workspace passed twice — all on the lab box with /dev/kvm rw. No divergence, no flake, no Critical defect.

### Important

_None._

### Suggestions

1. **Promote `kvm-ioctls` and `libc` to `[workspace.dependencies]`.** `kvm-ioctls = "0.24.0"` is a hand-edited literal in three Cargo.toml files (`crates/dh-vmm/Cargo.toml:15`, `tools/dh-cli/Cargo.toml:11`, `tests/determinism/Cargo.toml:14`); it is absent from the workspace dep table (`Cargo.toml:22`). Matching versions today, drift risk tomorrow. Switch all to `kvm-ioctls.workspace = true` / `libc.workspace = true`. Non-blocking hygiene; suitable for a follow-up bead.

2. **De-duplicate `gettid()`.** The same argless-syscall shim now lives in `crates/dh-vmm/src/{boundary.rs:204,inject.rs:171,inject.rs:320,run.rs:142}` and `tests/determinism/tests/regression.rs:36`. Export one `pub fn current_tid() -> i32` from dh-vmm and have the test plus the four internal copies call it.

3. **Structured mismatch message.** On divergence the assert prints two unlabeled `(u64,u64,u64,u64,[u8;32])` tuples. Add a comparison that names the first differing field (`icount/rip/rcx/vns/state_hash`) to speed P0 triage. `tests/determinism/tests/regression.rs:111-114`.

4. **Optional: drop or keep the no-op `#[allow(unsafe_code)]`** at `regression.rs:42` — there is no crate-level `forbid`/`deny(unsafe_code)` for it to suppress, so it currently does nothing (build is warning-clean). Harmless either way; only flagged for completeness.

### Note (tracking only, not an ask on this branch)

- Making the `kvm-intel` CI lane a **required status check** (the "required-for-merge from M3 onward" part of the bead) is GitHub branch-protection config, not a file in this diff. It's delegated to the dependent wiring bead. This branch correctly *lands* the test in the lane (`ci.yaml` kvm-intel runs `cargo test --workspace` with no exclude, and pre-checks /dev/kvm rw so it can't silently skip); only the required-check toggle remains, elsewhere.
