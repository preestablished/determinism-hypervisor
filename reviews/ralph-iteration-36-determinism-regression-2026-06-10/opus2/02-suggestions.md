# Suggestions (all minor, none blocking)

## S1 — `kvm-ioctls` version is a hardcoded string in 3 places (drift risk)

`kvm-ioctls = "0.24.0"` is repeated literally in `crates/dh-vmm/Cargo.toml:15`, `tools/dh-cli/Cargo.toml:11`, and now `tests/determinism/Cargo.toml:14`. It is **not** a `[workspace.dependencies]` entry (verified: the workspace dep table at `Cargo.toml:22` has no `kvm-ioctls`). The version matches dh-vmm today, so there's no current bug — but three hand-edited copies will drift. Suggest promoting `kvm-ioctls` to `[workspace.dependencies]` and using `kvm-ioctls.workspace = true` everywhere (dh-detclock/dh-vmm are already done this way). Same applies to `libc = "0.2.186"`. Pure hygiene; out-of-scope-able to a follow-up bead.

## S2 — Duplicated `gettid()` helper (4th copy)

The argless-syscall `gettid()` shim now exists verbatim in four places: `crates/dh-vmm/src/{boundary.rs:204, inject.rs:171, inject.rs:320, run.rs:142}` and this test's `regression.rs:36`. The test is a separate crate, so it can't trivially reach an internal helper — but dh-vmm already needs one. Consider exporting a small `pub fn current_tid() -> i32` (or a `#[doc(hidden)]` test helper) from dh-vmm so the test and the four internal copies converge on one definition. Reduces the surface where a future signedness/cast change has to be made consistently.

## S3 — Mismatch failure UX prints two opaque 5-tuples

On divergence the assert dumps two `(u64,u64,u64,u64,[u8;32])` tuples with no field labels — for a P0 the on-call would have to mentally map positions to `icount/rip/rcx/vns/state_hash`. A structured message (compare fields and name the first that differs, e.g. `state_hash` vs `rcx`) would make triage faster and immediately hint at *where* nondeterminism crept in (RCX drift ⇒ guest path divergence; state_hash-only drift ⇒ memory/epoch). Minor; the gate's job is to fail, and it does.

## S4 — Redundant `#[allow(unsafe_code)]` on `gettid()`

`regression.rs:42` carries `#[allow(unsafe_code)]`, but the crate has no `#![forbid(unsafe_code)]`/`#![deny(unsafe_code)]` to allow against (confirmed: no crate-level lint attr; clean build produces zero warnings). It is harmless and arguably good forward-defensive hygiene (if a workspace lint is added later this won't suddenly fail). Leave it or drop it — noting only that it's currently a no-op.

## S5 (note, not an ask) — "required-for-merge" branch protection is out of this diff

The bead text says the gate "becomes required-for-merge from M3 onward (wiring bead below)." Branch-protection / required-status-check config is GitHub repo settings, not in this branch's files. That is explicitly delegated to a dependent wiring bead, so it is **not** a gap here — flagging only so the reviewer trail records that turning `kvm-intel` into a *required* check still has to happen elsewhere for the M3 accept criterion to be fully closed.
