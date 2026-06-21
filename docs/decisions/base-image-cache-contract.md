# Decision: Worker base-image cache immutability contract

**Bead:** determinism-hypervisor-hdi - decided 2026-06-21

## Context

Worker image-cache entries are content-addressed by lowercase BLAKE3 hex. The
worker verifies each cache entry against the hash in `MachineConfig` before the
bytes reach boot or device setup.

Boot blobs were already copied into process memory with explicit caps. Base
images were different: the resolver verified the cache file and then handed a
read-only file descriptor to pv-blk for lazy reads. `O_RDONLY` prevents writes
through that descriptor, but another writer with access to the same inode can
still mutate, truncate, or grow the cache entry after verification. A large
sparse cache entry could also make base-image hashing unbounded.

## Decision

Base images resolved from the worker image cache are bounded, copied, and owned
by the worker runtime before pv-blk sees them.

- Cache entries remain keyed by lowercase BLAKE3 hex.
- `MAX_BASE_IMAGE_BYTES` is 512 MiB.
- Base-image cache entries larger than the cap fail before hashing.
- The resolver reads the complete base image into process-owned bytes while
  hashing those bytes.
- The resolver verifies the owned bytes against
  `MachineConfig.base_image_hash`.
- pv-blk receives a `FileBase` backed by the owned bytes, not by the mutable
  cache inode.

After CreateVm, RestoreSnapshot, Fork, or VerifyReplay resolves a base image,
later cache-file mutation cannot affect that runtime's pv-blk reads or
read-modify-write overlay population.

## Consequences

The memory cost is bounded per runtime, not by a global worker image-memory
quota. Multiple active runtimes can multiply the owned base-image memory cost;
that is accepted for this contract because active slots are separately bounded
and current reference artifacts are small relative to 512 MiB.

Future large-image support must introduce a new immutable streaming backend,
such as a reviewed fs-verity or sealed-file design, rather than silently
relaxing the cap or restoring lazy reads from mutable cache inodes.
