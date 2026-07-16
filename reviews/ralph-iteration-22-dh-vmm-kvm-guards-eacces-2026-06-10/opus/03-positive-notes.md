# Positive Notes

- **Probe matches the real access mode exactly.** I cross-checked `kvm-ioctls-0.24.0`
  (the locked version): `Kvm::new()` → `open_with_cloexec(true)` →
  `open_with_cloexec_at(..)` with `open_flags = O_RDWR | O_CLOEXEC`
  (`system.rs:42-44, 128`). The guard's `.read(true).write(true)` is precisely `O_RDWR`,
  the access-permission component the kernel checks for `EACCES`. `O_CLOEXEC` is a
  file-descriptor flag, not an access bit, so omitting it from the probe is correct and
  does not change whether the open is permitted. The probe is the right predicate for
  "will `KvmSystem::open` succeed against the device permissions."

- **No fd leak or lingering side effect.** `OpenOptions::open(..)` returns
  `io::Result<File>`; `.is_ok()` consumes the temporary `File` immediately, so the
  descriptor is closed at the end of the expression. Opening `/dev/kvm` rw has no
  KVM-level side effect (no VM/vCPU created — that requires ioctls), so the probe is
  idempotent and cheap.

- **Correctly handles the cross-crate `cfg(test)` constraint.** The author recognized
  that a `#[cfg(test)]` helper in `dh-vmm` cannot be referenced from `dh-worker`'s test
  module and inlined the equivalent probe there, rather than producing a broken
  `crate::...` reference or leaking a non-test symbol. The inline comment documents the
  reasoning.

- **Centralized within `dh-vmm`.** The three same-crate modules (`kvm`, `msr`, `run`)
  now share one source of truth (`kvm_usable`) instead of three independent existence
  checks, so the access semantics can't drift across them.

- **Good, intent-revealing comments.** Both the `kvm_usable` doc comment and the
  `dh-worker` inline comment explain *why* existence is insufficient (hosted runners
  expose the node but deny access) and name where the live legs still gate (lab box,
  kvm-intel lane). This is exactly the context a future maintainer needs.

- **Verified green locally by the reviewer:** `cargo test -p dh-vmm -p dh-worker` runs
  the live legs (`caps_gate_passes_on_compliant_host`, `slot_vm_constructs_with_memfd_and_vcpu`,
  `full_preflight_passes_on_configured_host` all executed and passed — not skipped),
  `clippy --all-targets -D warnings` clean, `cargo fmt --check` clean. The branch does
  not over-skip on an rw-capable box.

- **Minimal blast radius.** +25/−5 across 4 files, test-only code paths, no production
  behavior touched.
