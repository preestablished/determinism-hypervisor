# Implementation Sequence

## Phase 1: Unblock And Claim

Run Beads commands serially; the embedded Dolt backend can lock on concurrent
read/write commands.

```bash
bd show determinism-hypervisor-hdi
bd update determinism-hypervisor-hdi --status open \
  --append-notes "Unblocking with accepted bounded copy-once base-image contract: resolver copies verified base bytes into owned immutable FileBase backing before pv-blk reads; MAX_BASE_IMAGE_BYTES will bound hashing and memory."
bd update determinism-hypervisor-hdi --claim
```

If another assignee has claimed the bead, stop and coordinate instead of
stealing ownership.

## Phase 2: Extend FileBase With Owned Bytes

File: `crates/dh-vmm/src/blkfile.rs`

Refactor `FileBase` so it can serve either a file or immutable owned bytes.
Keep the public `FileBase` type and existing `FileBase::open` /
`FileBase::from_file` APIs.

Suggested shape:

```rust
pub struct FileBase {
    backing: BaseBacking,
    len: u64,
}

enum BaseBacking {
    File(File),
    Bytes(std::sync::Arc<[u8]>),
}
```

Add:

```rust
pub fn from_bytes(bytes: Vec<u8>) -> FileBase
```

or:

```rust
pub fn from_owned_bytes(bytes: Vec<u8>) -> FileBase
```

The exact name is less important than making the invariant clear in comments:
the worker resolver uses this constructor when it needs bytes detached from the
mutable image cache.

Update `BlockBase::read_at`:

- File backing: preserve current `read_exact_at` behavior and zero-fill past EOF.
- Byte backing: copy from the slice and zero-fill past EOF.

Update module comments. Do not continue claiming that read-only file open makes
the base immutable by construction. Say that file-backed `FileBase` is useful
for fixtures and trusted direct paths, while worker cache resolution uses owned
bytes to satisfy the immutable `BlockBase` contract.

## Phase 3: Add Base Image Cap And Owned Resolution

File: `crates/dh-worker/src/image_resolver.rs`

Add:

```rust
pub const MAX_BASE_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
```

Change `ImageBlobKind::boot_blob_limit` to a more general limit helper, or add
a separate `base_image_limit` path. Keep boot blob caps unchanged.

Add an allocation-specific resolver error:

```rust
AllocationFailed {
    kind: ImageBlobKind,
    path: PathBuf,
    requested: u64,
}
```

The exact variant name can differ, but memory pressure while copying a bounded
base image must be an ordinary `Result` error. Do not use `Vec::with_capacity`
or unchecked growth for potentially large cache entries. Use
`Vec::try_reserve_exact` before the read loop, or a chunked fallible growth
pattern that uses `try_reserve` before extending the buffer.

Update `open_base_image`:

1. Open the cache file with `open_cache_file`.
2. Reject `len > MAX_BASE_IMAGE_BYTES` with `ImageResolverError::TooLarge`.
3. Fallibly reserve at most `len` bytes for the owned buffer.
4. Read the file into a `Vec<u8>` while hashing.
5. Guard against files that grow during read:
   - track `total` bytes read;
   - if `total > MAX_BASE_IMAGE_BYTES`, return `TooLarge`;
   - use the actual read total in any `TooLarge` error.
6. Compare computed hash against `expected`.
7. Return `Ok((path, FileBase::from_owned_bytes(out)))` or the equivalent
   constructor name.

Prefer factoring shared read-and-hash code so boot blobs and base-image reads
do not drift:

```rust
fn read_verified_blob_limited(
    &self,
    kind: ImageBlobKind,
    expected: &[u8; 32],
    max_bytes: u64,
) -> Result<(PathBuf, Vec<u8>), ImageResolverError>
```

Then:

- `read_blob_limited` can return only the bytes from that helper.
- `open_base_image` can call the helper and wrap the bytes in `FileBase`.
- the helper must check metadata length before allocation and before any
  read/hash loop. This is load-bearing for sparse oversized files.

Keep `open_verified_file` only if another caller still needs it. If it becomes
unused, delete it and let tests compile-check that no path still hands mutable
cache fds to pv-blk.

## Phase 4: Preserve Service Error Mapping

File: `crates/dh-worker/src/service.rs`

`ImageResolverError::TooLarge` already maps to `Status::invalid_argument`.
Keep that mapping. The implementation should not turn base-image cap failures
into `Unavailable` or `DataLoss`.

Map the new allocation failure variant to `Status::resource_exhausted`. Include
the kind, path, and requested byte count in the error text so an operator can
distinguish memory pressure from hash mismatch or missing cache entries.

Audit all `resolve_runtime_base_image` call sites:

- CreateVm
- RestoreSnapshot
- Fork
- VerifyReplay

They should all automatically receive the owned immutable base through
`ImageResolver::open_base_image`.

## Phase 5: Keep The Change Narrow

Do not change:

- `MachineConfig.base_image_hash` encoding.
- pv-blk overlay serialization.
- M9 artifact hashing.
- boot blob caps except for refactoring shared helpers.
- cache population helpers in tests except where new cap/mutation tests need
  utilities.

If a large refactor seems necessary, stop and split a follow-up bead. This
bead should be a small contract fix with focused tests.
