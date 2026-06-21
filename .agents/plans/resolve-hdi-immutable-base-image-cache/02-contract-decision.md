# Contract Decision

## Decision

Base images resolved from the worker image cache are immutable runtime inputs.
The worker must not let pv-blk read directly from the cache inode after hash
verification. Instead, the worker resolves a base image by:

1. Opening the content-addressed cache entry with the existing no-symlink,
   regular-file checks.
2. Checking metadata length against `MAX_BASE_IMAGE_BYTES`.
3. Reading the full file into process-owned bytes while computing BLAKE3.
4. Rejecting if the computed hash differs from the requested
   `MachineConfig.base_image_hash`.
5. Creating the pv-blk base from those owned bytes.

After step 5, cache mutations are irrelevant to that VM. The cache is only a
source for a verified copy; it is not the live pv-blk backing store.

## Size Cap

Add this constant in `crates/dh-worker/src/image_resolver.rs`:

```rust
pub const MAX_BASE_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
```

Rationale:

- The current M9 reference artifacts are tiny relative to this cap.
- 512 MiB is high enough for current deterministic fixture and game-image
  use, while still bounding hash time and owned-memory pressure.
- The cap is explicit and test-pinned; future larger workloads must make a
  conscious contract change instead of growing unbounded by accident.

If implementation discovers an existing checked-in test fixture or documented
reference artifact larger than 512 MiB, stop and update the plan or bead before
changing the cap.

## Why Not Cache Permissions

Root-owned or read-only cache directories are useful operational hardening, but
they do not give a self-contained worker invariant. Tests running as the same
user can still model inode mutation, and deployments can drift. The worker
should fail closed or detach from mutable cache state on its own.

## Why Not fs-verity

fs-verity would provide a strong kernel-backed immutable content proof, but it
depends on filesystem support, provisioning, and operator policy. This repo's
unit tests and local worker harnesses should be able to prove the contract
without requiring root or a specific filesystem.

## Why Not Sealed memfd In This Bead

A sealed memfd copy would also solve the inode mutation problem, but it needs
Linux syscall plumbing. `dh-worker` forbids unsafe code, and `dh-vmm` currently
keeps memfd/seal unsafe localized to x86_64 KVM internals. The owned-byte
backing gives the same immutability property for pv-blk reads with no unsafe
and works in portable tests.

If future workloads require streaming from a sealed file instead of holding
bounded bytes in memory, file a follow-up bead for a dedicated immutable
backend. Do not mix that larger design into `hdi`.

## Accepted Runtime Tradeoff

CreateVm, RestoreSnapshot, Fork, and VerifyReplay may spend bounded time and
memory copying the base image when constructing a runtime bus. This is accepted
for correctness. The copy cost is bounded by `MAX_BASE_IMAGE_BYTES` and should
be visible through existing CreateVm/restore/fork/replay latency tests if it
regresses badly.

The cap is per runtime, not a global worker memory quota. Multiple concurrent
VMs can multiply the owned base-image memory cost. That is acceptable for this
bead because the current reference artifacts are small, active slots are already
bounded by worker configuration, and this change closes the mutation hole
without adding host-specific cache requirements. If global image-memory quotas
become necessary, file a follow-up bead.
