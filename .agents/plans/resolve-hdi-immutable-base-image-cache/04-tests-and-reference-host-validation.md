# Tests And Reference Host Validation

## Pure Unit Tests

Run focused tests while implementing:

```bash
cargo test -p dh-vmm blkfile
cargo test -p dh-worker image_resolver
```

Add or update these tests.

### FileBase Owned Bytes

File: `crates/dh-vmm/src/blkfile.rs`

Add a test that constructs `FileBase::from_bytes` and proves:

- `len()` reports the byte length.
- `read_at` returns the expected bytes.
- reads that extend past EOF zero-fill the tail.
- the owned backing works through the `BlockBase` trait.

Keep the existing file-backed tests intact.

### Post-Verification Cache Mutation

File: `crates/dh-worker/src/image_resolver.rs`

Add a test like:

```rust
#[test]
fn base_image_is_owned_after_verification() {
    let cache = CacheDir::new("base-owned-after-verify");
    let original = b"base image original bytes ...";
    let hash = cache.write_blob(original);

    let (_path, base) = ImageResolver::new(&cache.path)
        .open_base_image(&hash)
        .unwrap();

    cache.write_at_hash(&hash, b"same path, different bytes ...");

    let mut got = vec![0; original.len()];
    base.read_at(0, &mut got).unwrap();
    assert_eq!(got, original);
}
```

Use equal-length replacement bytes if convenient, but the test should also
make clear that truncation/growth of the cache file after resolution cannot
change the already constructed `FileBase`.

Also add a pv-blk-level test, because the bead is about safety before and
during device reads, not only direct `FileBase::read_at`:

- Resolve a base image through `ImageResolver::open_base_image`.
- Build `dh_devices::blk::PvBlk` from the returned base.
- Mutate the cache entry after resolution. Cover at least one overwrite and one
  truncate-or-grow case.
- Issue a pv-blk read and prove the guest buffer receives the original owned
  bytes.
- Issue a pv-blk write that triggers cluster read-modify-write, then read back
  an untouched sector from the same cluster and prove RMW used original owned
  base bytes, not the mutated cache file.

This can live in `image_resolver.rs` tests if importing `PvBlk`, `VecGuestMem`,
and the existing request helper is small. If it grows too much, put it in a
focused worker integration test. The assertion must exercise the real `PvBlk`
device path.

### Too-Large Sparse Base Image

File: `crates/dh-worker/src/image_resolver.rs`

Add a sparse-file test:

```rust
#[test]
fn base_image_cap_rejects_sparse_entry_before_hashing() {
    let cache = CacheDir::new("base-too-large");
    let expected = [0x65; 32];
    cache.create_sparse_at_hash(&expected, MAX_BASE_IMAGE_BYTES + 1);

    match ImageResolver::new(&cache.path).open_base_image(&expected) {
        Err(ImageResolverError::TooLarge {
            kind: ImageBlobKind::BaseImage,
            len,
            max,
            ..
        }) => {
            assert_eq!(len, MAX_BASE_IMAGE_BYTES + 1);
            assert_eq!(max, MAX_BASE_IMAGE_BYTES);
        }
        other => panic!("wrong result: {other:?}"),
    }
}
```

This test must not hash the sparse file. The code should reject from metadata
length first.

Add a small-limit helper-level test after factoring `read_verified_blob_limited`:

- Create a sparse file of about 1 MiB at a content-addressed path.
- Call the helper with `max_bytes = 4`.
- Assert the error is `TooLarge` and `len` equals the metadata length, not
  `max_bytes + 1`.
- Code review must confirm the helper checks `len > max_bytes` before
  allocation and before any `read`/hash loop. This is what prevents sparse
  files from becoming unbounded hash work.

### Fallible Allocation

File: `crates/dh-worker/src/image_resolver.rs`

Add a narrow test around the allocation failure path if it can be induced
without destabilizing the test process. If direct allocation failure is not
practical, keep the code path small and explicitly review that it uses
`try_reserve_exact` or `try_reserve` and returns `ImageResolverError` instead
of panicking or aborting. The service mapping test below must cover the
allocation error variant directly.

### Base Hash Mismatch Still Reports Data Loss Upstream

At the resolver level, add a base-image hash mismatch test if one does not
already exist. It should return `ImageResolverError::HashMismatch` with
`kind == ImageBlobKind::BaseImage`.

At the service level, add explicit base-image status-code coverage. Generic
kernel/initramfs mapping tests are not enough for this bead. Cover:

- `HashMismatch { kind: BaseImage }` maps to `data_loss`.
- `TooLarge { kind: BaseImage }` maps to `invalid_argument`.
- `NotFile { kind: BaseImage }` maps to `failed_precondition`.
- allocation failure for `BaseImage` maps to `resource_exhausted`.

It is acceptable to test the private `image_error_to_status` helper from
`service.rs` unit tests if a full CreateVm failure setup would be noisy. A
CreateVm-level test is better if it stays focused.

## Workspace Validation

Before closing the bead:

```bash
cargo fmt --check
cargo test -p dh-vmm blkfile
cargo test -p dh-worker image_resolver
cargo test -p dh-worker --lib
cargo test --workspace
```

If `cargo test --workspace` is too slow only because of unrelated live-KVM
acceptance tests, do not silently skip it. Record the exact failure and run the
relevant workspace subset, but the preferred closeout is a full workspace pass.

## Linux/KVM Reference Host Evidence

This repo is currently on the Linux/KVM reference host. After unit and workspace
tests pass, run at least one no-skip Linux gate that exercises the worker image
resolver and pv-blk path with the staged M9 artifacts.

Preferred command:

```bash
DH_M9_ALLOW_SKIP=0 \
cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture
```

If time is tight, this narrower pv-blk command is acceptable because it
directly exercises Linux pv-blk replay:

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture
```

Do not use `DH_M9_ALLOW_SKIP=1` as acceptance evidence. If the reference host
loses KVM or the staged artifacts are missing, leave the bead open with a
comment and the exact blocker.
