# Resolve HDI Immutable Base-Image Cache Contract

Plan name: `resolve-hdi-immutable-base-image-cache`

Selected bead: `determinism-hypervisor-hdi` - Decide and enforce immutable bounded base-image cache contract.

## Why This Bead

`determinism-hypervisor-hdi` is a real local blocker with a security and
determinism failure mode. The worker image resolver verifies the base-image
hash and then hands a read-only fd to pv-blk for lazy reads. A read-only fd
does not make the inode immutable: another writer with access to the same cache
entry can modify bytes after verification, so later pv-blk reads can observe
bytes that were not hashed into `MachineConfig.base_image_hash`.

The second blocker is resource bounding. Boot blobs already have caps, but base
images do not. A huge or sparse cache entry can make CreateVm spend unbounded
time hashing and, with a copy-backed fix, can also consume unbounded memory.

This is a good next bead because it is self-contained in this repo and can be
proved on the Linux/KVM reference host. It does not require the unreachable
upstream planning tree and does not depend on a human-only external decision if
the implementation follows this plan's chosen contract.

## Chosen Contract

Use a bounded, copy-once, process-owned base-image backing:

- Add an explicit `MAX_BASE_IMAGE_BYTES` cap in `crates/dh-worker/src/image_resolver.rs`.
- When resolving a base image, reject entries whose metadata length exceeds the
  cap before hashing.
- Read the complete base image into an owned byte buffer while hashing.
- Verify the owned bytes against `MachineConfig.base_image_hash`.
- Build the pv-blk `FileBase` from the owned bytes, not from the cache inode.

This intentionally trades bounded memory and startup cost for a simple,
portable, deterministic immutability guarantee. After `CreateVm`,
`RestoreSnapshot`, `Fork`, or `VerifyReplay` resolves the base image, later
cache-file mutations cannot affect pv-blk reads for that runtime.

## Non-Goals

Do not introduce a trusted mutable cache-writer contract, fs-verity dependency,
root-owned cache permissions, or Linux-only sealed-memfd requirement in this
bead. Those options either depend on host configuration outside this repo or
require unsafe/syscall plumbing. If future workloads need very large base
images without an in-memory copy, file a follow-up for a streaming immutable
backend such as fs-verity or sealed memfd. Do not silently keep the current
lazy inode-backed behavior for base images.

## Desired End State

The implementation agent leaves the repo in this state:

- `determinism-hypervisor-hdi` is unblocked, claimed, implemented, tested,
  and closed.
- `FileBase` can serve either an opened file or owned immutable bytes.
- Worker base-image resolution uses owned immutable bytes and enforces an
  explicit maximum size.
- Worker base-image resolution uses fallible allocation; memory pressure fails
  with an actionable resolver/service error instead of aborting the process.
- Tests prove post-verification cache mutation cannot affect pv-blk reads.
- Tests prove too-large sparse base-image entries fail closed before hashing.
- CreateVm/Restore/Fork/VerifyReplay error mapping stays actionable.
- Documentation records the accepted bounded base-image cache contract.
- `cargo fmt --check`, focused tests, and `cargo test --workspace` pass.
- On this Linux/KVM reference host, the implementation agent also runs at least
  one no-skip Linux pv-blk or worker API gate that exercises the real resolver.
- Beads and Git are pushed before handoff.

## File Map

- `01-current-state.md` records the existing code paths and blocker.
- `02-contract-decision.md` states the exact accepted contract and rejected alternatives.
- `03-implementation-sequence.md` gives file-level implementation steps.
- `04-tests-and-reference-host-validation.md` lists required tests and KVM evidence.
- `05-docs-and-error-mapping.md` covers documentation and API error behavior.
- `06-beads-and-closeout.md` gives Beads, commit, push, and handoff steps.
- `07-review-resolution.md` summarizes subagent review feedback and final edits.
- `08-review-security-correctness.md` contains the security/correctness review.
- `09-review-implementation-feasibility.md` contains the feasibility/reference-host review.
