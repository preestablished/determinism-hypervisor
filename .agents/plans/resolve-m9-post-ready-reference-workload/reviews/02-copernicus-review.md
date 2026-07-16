# Subagent Review 2 - Copernicus

Verdict: `REQUEST_CHANGES`

## Critical Issues

- `4s9.30` is not blocked only by the smoke manifest. The bead notes also
  record a pre-READY `VerifyReplay` divergence, but the plan treated replay
  divergence as reactive debugging after fixture replacement instead of an
  owned unblock step. Add an explicit phase: rerun `linux_worker_api`, classify
  divergence as fixture-vs-hypervisor, and file/claim the owning work before
  `4s9.30` can close.
- Fixture-builder ownership was ambiguous. The plan needs a named owner
  boundary: what this repo implements, what the external fixture builder must
  produce, where the external issue/release SHA is tracked, and what happens if
  the guest-side `/dev/vdb` shim or workload ABI is missing.

## Important Issues

- Gate ordering conflicted. Split `linux_worker_api` into a manifest/READY/region
  preflight gate and a full close gate including `Fork` and `VerifyReplay`.
- The post-READY workload ABI was under-specified. The fixture contract should
  define how the guest exposes a stable loop, IF-enabled interrupt window,
  frame marks, and IO phase without host input.
- Several acceptance commands can pass as zero-test filters unless the
  implementation adds Linux-named tests. Require evidence that each Linux case
  actually ran and did not skip.
- `4s9.27` corpus acceptance omitted checked-in/documented corpus metadata:
  expected hashes, determinism-class lock reference, and fixture README policy.

## Suggestions

- Add `crates/dh-vmm/src/kvm.rs` and config/proto mapping seams to local
  authority for forbidden timer/irqchip assertions.
- Add the M7 cross-slot same-seed command and nightly 100-child canary to the
  acceptance-gates file.
- Keep `assert_initramfs_boot_contract` strict; it is the right first fast-fail
  gate.
