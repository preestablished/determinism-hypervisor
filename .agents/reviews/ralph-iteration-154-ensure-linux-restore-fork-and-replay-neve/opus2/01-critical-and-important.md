# Critical And Important Issues

## Critical

No critical issues found.

## Important

### Important: Restore, replay, and fork still require unused boot blobs

- File: `crates/dh-worker/src/service.rs:1482`, `crates/dh-worker/src/service.rs:3380`, `crates/dh-worker/src/service.rs:3496`

The branch correctly removes `boot_slot(...)` from `VerifyReplay` and `RestoreSnapshot`, but those paths still call `image_resolver.resolve_create_vm(&config)`. That resolver always reads and verifies the boot blobs from the recovered config, including BzImage kernel and initramfs bytes, before returning the base image. After this change those boot bytes are unused for restore/replay/fork, so a snapshot-only operation can still fail because a kernel or initramfs cache entry is missing, corrupt, too large, or slow to load.

This matters for the bead acceptance because it validates "no second Linux boot", but it does not validate that restored/replayed state is independent of the boot artifacts once the initial snapshot exists. The new M9 helpers always populate kernel, initramfs, base image, and game image together, so they do not catch this implicit dependency.

Suggested fix: split runtime base-image resolution from create-VM boot resolution. Keep `resolve_create_vm` for `CreateVm`, but have restore/replay/fork validate the recovered config and open only the base image.

```rust
#[cfg(target_arch = "x86_64")]
fn resolve_runtime_base_image(
    image_resolver: &crate::image_resolver::ImageResolver,
    config: &dh_vmm::config::MachineConfig,
) -> Result<dh_vmm::blkfile::FileBase, Status> {
    config
        .validate()
        .map_err(crate::image_resolver::ImageResolverError::InvalidConfig)
        .map_err(image_error_to_status)?;

    let (_path, base_image) = image_resolver
        .open_base_image(&config.base_image_hash)
        .map_err(image_error_to_status)?;
    Ok(base_image)
}
```

Then use that helper on no-boot paths:

```rust
let base_image = resolve_runtime_base_image(&image_resolver, &config)?;
let bus = build_bus(
    &config,
    base_image,
    RuntimeVmMem(slot.guest_mem.clone()),
)?;
```

Also add a regression test that creates the Linux READY snapshot, removes or renames only the cached kernel/initramfs entries, and verifies `RestoreSnapshot` and `VerifyReplay` still succeed from the snapshot.

### Important: M9 cache helper can clobber hash-keyed cache entries under concurrent tests

- File: `crates/dh-worker/tests/common/mod.rs:205`

`ensure_cache_entry` checks `dest.exists()`, then tries `hard_link`, then falls back to `std::fs::copy(source, &dest)` on any link error. If two ignored M9 tests populate the same `DH_M9_IMAGE_CACHE` concurrently, one process can create `dest` after another process's `exists()` check but before its copy fallback. `std::fs::copy` truncates and overwrites the destination, so a valid hash-keyed cache entry can be replaced while another worker is reading it.

The helper is test-only, but it is shared by the new M9 acceptance tests and writes into a user-provided cache root. A race here can produce flaky acceptance failures or, worse, mutate a cache entry that is expected to be immutable by hash.

Suggested fix: treat `AlreadyExists` as success only after re-hashing, and publish copied data through a temporary file plus non-clobbering link or rename behavior.

```rust
pub fn ensure_cache_entry(source: &Path, cache_root: &Path) -> TestResult<[u8; 32]> {
    let hash = hash_file(source)?;
    let key = dh_worker::image_resolver::cache_key(&hash);
    let dest = cache_root.join(&key);

    if dest.exists() && hash_file(&dest)? == hash {
        return Ok(hash);
    }

    match std::fs::hard_link(source, &dest) {
        Ok(()) => return Ok(hash),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if hash_file(&dest)? == hash {
                return Ok(hash);
            }
            return Err(format!("image cache entry {} exists with wrong hash", dest.display()));
        }
        Err(_) => {}
    }

    let tmp = cache_root.join(format!("{key}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    std::fs::copy(source, &tmp)
        .map_err(|e| format!("copy {} to temp cache {}: {e}", source.display(), tmp.display()))?;
    if hash_file(&tmp)? != hash {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("image cache temp entry {} hash mismatch", tmp.display()));
    }

    match std::fs::hard_link(&tmp, &dest) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && hash_file(&dest)? == hash => {}
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("publish image cache entry {}: {e}", dest.display()));
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Ok(hash)
}
```
