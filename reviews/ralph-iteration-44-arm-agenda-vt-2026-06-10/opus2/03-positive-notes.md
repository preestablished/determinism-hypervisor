# Positive Notes

- **Surgical, output-preserving rename.** The sed only rewrote `dh_cli:: -> crate::` in code
  paths; every byte of user-facing output (usage text, JSON keys, error prefixes) survived
  unchanged. Verified by diffing against `main:` and by running the binary live. This is exactly
  the discipline a script-consumed CLI needs.

- **Clean portability boundary.** The portable set (agenda, vt, config, blkfile) genuinely has no
  KVM dependencies — confirmed by grep AND by a full aarch64 cross-compile + clippy run, not by
  inspection alone. config.rs's `CpuidLeaf` being a plain local struct (not a kvm_bindings
  re-export) is what lets the whole MachineConfig surface stay ungated.

- **Honest non-x86 failure, not a silent skip.** dh-worker's `kvm_checks()` stub returns
  `ok=false` with `got=target_arch=<arch>` / `want=x86_64 (VMX)` rather than vanishing — so an
  accidental arm preflight reports a clear reason instead of a misleading green. Good fail-loud
  instinct for a determinism product.

- **Cargo manifests target-gated at the right granularity.** kvm-bindings/kvm-ioctls/libc/
  vm-memory/vmm-sys-util and the nanokernel dev-dep moved under the cfg target block, while the
  portable `dh-vmm.workspace` / shared crates stay in the plain `[dependencies]`. The nanokernel
  dep in dh-cli correctly landed in the gated block because its only user (skid.rs) is gated.

- **CI comment is accurate and load-bearing.** `.github/workflows/ci.yaml:33-37` precisely
  describes the new arrangement ("the arm lane tests the pure determinism math ... the live-KVM
  tests compile to empty"), and the build.rs comment block (nanokernel/build.rs:5-9) already
  documents the linker fallback chain that makes the arm lane viable.

- **The bead's stated highest-risk failure mode was actively avoided.** A gated dev-dep
  consumed by an ungated test would have broken the arm lane; the portable modules' test blocks
  are provably free of nanokernel/kvm usage, and the cross-compile of `--all-targets` proves it.

- **All consumers stay inside their gate.** Every reference to a gated dh-vmm module (10 call
  sites across dh-cli + 1 in dh-worker) sits inside an already-gated module — no leakage, which
  is why the adversarial cross-build is green.
