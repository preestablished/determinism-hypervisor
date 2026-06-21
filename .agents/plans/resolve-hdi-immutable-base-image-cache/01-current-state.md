# Current State

## Bead

`bd show determinism-hypervisor-hdi` reports:

- Status: `BLOCKED`
- Priority: `P2`
- Type: `task`
- Labels: `image-cache`, `review`, `security`, `worker`

The bead was filed from a review finding while working M9 image resolver
coverage. It is explicitly not part of `4s9.19`, which only covered BzImage and
initramfs boot blobs.

The blocker text says the project must choose and implement a deterministic
base-image strategy. The accepted implementation must prove one of these:

- base-image bytes cannot change after resolver verification and before/during
  pv-blk reads; or
- the resolver fails closed.

It must also test a too-large/sparse base image and keep CreateVm error mapping
actionable.

## Code Paths

Primary files:

- `crates/dh-worker/src/image_resolver.rs`
- `crates/dh-vmm/src/blkfile.rs`
- `crates/dh-devices/src/blk.rs`
- `crates/dh-worker/src/service.rs`

Current resolver behavior:

- Boot blobs are read into `Vec<u8>` with caps:
  - `MAX_KERNEL_BYTES`
  - `MAX_INITRAMFS_BYTES`
- Cache entries are opened with `O_NOFOLLOW | O_NONBLOCK`.
- Cache entries must be regular files.
- Hash mismatch is reported as `ImageResolverError::HashMismatch`.
- Too-large boot blobs are rejected with `ImageResolverError::TooLarge`.
- Base images use `open_base_image`, which calls `open_verified_file`, hashes
  the opened file, then hands that same fd to `FileBase::from_file`.

Current `FileBase` behavior:

- `FileBase::open` opens the path read-only.
- `FileBase::from_file` stores the file and its metadata length.
- `BlockBase::read_at` uses `pread` on the file and zero-fills past EOF.

Current pv-blk contract:

- `dh_devices::blk::BlockBase` says implementations must be immutable and
  deterministic: same offset to same bytes forever.
- `PvBlk` reads base bytes lazily through `BlockBase` and stores guest writes
  in a CoW overlay.

The mismatch is that `FileBase`'s read-only fd does not make the underlying
inode immutable. The current comments say the base is immutable by construction,
but that is only true for writes through the fd itself, not for another writer
with access to the cache entry.

## Constraints

- `dh-worker` has `#![forbid(unsafe_code)]`.
- `dh-vmm` has `#![deny(unsafe_code)]` and already localizes syscall unsafe in
  x86_64 KVM modules.
- `dh-devices` cannot perform host I/O.
- The fix should keep portable unit tests working without KVM.
- The Linux/KVM reference host is available for final integration evidence.

## Existing Tests To Preserve

At minimum preserve these existing behaviors:

- `cargo test -p dh-worker image_resolver`
- `cargo test -p dh-vmm --test blk_fixture`
- `cargo test -p dh-vmm blkfile`
- `cargo test --workspace`

The current M9 Linux worker tests also depend on image-cache resolution:

- `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture`
- `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture`

Use one of those Linux/KVM commands as reference-host evidence after the pure
unit tests pass.
