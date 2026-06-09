# determinism-hypervisor
Deterministic hypervisor; KVM VMM; x86_64 Linux only

## Workspace layout

The Cargo workspace follows `.agents/docs/determinism-hypervisor/ARCHITECTURE.md`
section 1:

- `crates/dh-vmm`: core VMM library. It owns the former `dh-types` slot-state
  scaffold and the former `dh-kvm` capability-check scaffold.
- `crates/dh-detclock`: guest instruction counter and PMI boundary timing home.
- `crates/dh-devices`: deterministic device-model home.
- `crates/dh-inputlog`: DHILOG core. Its dependency set is intentionally limited
  to `blake3`.
- `crates/dh-snapshot`: snapshot codec and dirty-page tracking home.
- `crates/dh-verify`: determinism verification and diagnostics home.
- `crates/dh-proto`: thin wrappers over the sibling
  `../control-plane/crates/determinism-proto` path dependency.
- `crates/dh-worker`: daemon layer. It may depend on all workspace crates;
  no workspace crate depends on it.
- `tools/dh-cli`: local debug CLI.
- `tests/nanokernel` and `tests/determinism`: architecture test homes.

Disposition of the initial scaffold-only crates:

- `dh-types` was folded into `dh-vmm`; shared public types should be introduced
  only when a later architecture section requires them outside the VMM boundary.
- `dh-kvm` was folded into `dh-vmm`; KVM setup and capability policy are part of
  the VMM core in ARCH section 1.
- `dh-smoke` was retired as a crate; its smoke assertion moved into `dh-worker`
  package tests, with `tests/determinism` reserved for end-to-end gates.
