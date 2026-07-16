# Critical & Important Findings

## Critical

**None.**

The change is a build-system/cfg refactor with no semantic effect on x86_64. Determinism (the product) is unaffected: the only x86-visible diffs are module reorganization (lib.rs declaration order — no runtime effect), a dh-worker cfg attribute plus a new non-x86 function, and a behavior-preserving git-mv of the dh-cli CLI. Verified the moved CLI is byte-identical except `dh_cli::`→`crate::` and `fn main`→`pub fn main`; verified dh-worker's x86 `kvm_checks()` body has zero deletions.

## Important

**None.**

All claimed verification reproduced:
- x86_64: fmt/clippy clean, 195 tests pass (no test functions added or removed by the diff — coverage preserved).
- aarch64: `cargo check`/`clippy --workspace --all-targets` clean.
- The arm lane's actual value is real: agenda (9), vt (8), config (7), blkfile (2) unit tests are ungated `#[cfg(test)]` modules and run on arm; the gated KVM test targets compile to empty there.
- nanokernel stays a workspace member, builds on arm (nasm cross-assembles x86); dev-dep gating in dh-vmm doesn't remove it from the workspace.
- Cargo.lock unchanged → x86 kvm-intel lane resolves identically; no feature-unification surprise (dh-devices does not pull vm-memory).
- ci.yaml comment accurate, nasm install retained, no stale exclude references.
