# Review Overview

- **Branch:** `ralph/iteration-22-dh-vmm-kvm-guards-eacces`
- **Base:** `main`
- **Beads issue:** determinism-hypervisor-vwr
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus

## Summary

This branch fixes a CI failure (run 27251695143) where GitHub-hosted `ubuntu-latest`
runners expose `/dev/kvm` (nested virt) but deny the runner user access. The existing
test guards in `dh-vmm` (`kvm.rs`, `msr.rs`, `run.rs`) and `dh-worker` (`preflight.rs`)
gated on node *existence* (`Path::new("/dev/kvm").exists()`), so the live-KVM tests ran
and failed `EACCES` on those runners. The change replaces existence checks with an
*rw-open* probe: a single crate-level `#[cfg(test)] pub(crate) fn kvm_usable()` in
`dh-vmm/src/kvm.rs` that attempts `OpenOptions::new().read(true).write(true).open("/dev/kvm")`,
with the three per-module `kvm_available()` helpers delegating to it. `dh-worker`'s
`full_preflight_passes_on_configured_host` test gets the equivalent rw-open inline,
because `#[cfg(test)]` items are not visible across crate boundaries. The probe is
semantically correct — I confirmed against `kvm-ioctls-0.24.0` that `Kvm::new()`
(used by `KvmSystem::open`) opens with `O_RDWR | O_CLOEXEC`, so an rw open is exactly
the access mode the live path needs, and the probe `File` is dropped immediately with
no fd leak. The diff is small, focused, and well-commented.

## Verdict

**APPROVE**

The change is correct, minimal, and verified. There is one *Important* observation
about a pre-existing CI gate inconsistency (`ci.yaml:94` uses read-only `test -r`,
now weaker than the rw probe) that is out of this diff's scope but worth a follow-up,
plus minor suggestions. None block merge.

## Stats

- Files changed: 4
- Lines: +25 / −5
- Commits: 1 (`57fb11e`)
- New public(crate) test helpers: 1 (`kvm_usable`)
- Verification performed by reviewer: tests build + run (live legs executed, not
  skipped), `clippy -D warnings` clean, `cargo fmt --check` clean, probe semantics
  cross-checked against `kvm-ioctls-0.24.0` source.
