# Critical And Important Issues

No critical issues found.

## Important

### Important: restore/replay/fork still resolve unused boot blobs

- File: `crates/dh-worker/src/service.rs:1482`, `crates/dh-worker/src/service.rs:3380`, `crates/dh-worker/src/service.rs:3496`
- Related file: `crates/dh-worker/src/image_resolver.rs:156`

`RestoreSnapshot` and `VerifyReplay` no longer call `boot_slot`, which is the right direction. However, both paths still call `ImageResolver::resolve_create_vm`, and `Fork` already does the same inside the child loop. `resolve_create_vm` opens the base image but also reads and hashes the boot kernel and, for Linux, the initramfs (`image_resolver.rs:164-181`). Those boot bytes are then dropped on restore/replay/fork because only `assets.base_image` is passed into `build_bus`.

This leaves a stale boot-artifact dependency on paths that should be restoring or cloning already-materialized VM state. A restore, replay, or fork can still fail if the kernel/initramfs cache entries were garbage-collected even though no boot initialization is needed. It also adds avoidable I/O, hashing, and allocation on hot paths; for Linux fork this can repeat for every child.

Suggested fix: split the image resolver into create-time assets and runtime bus assets, then use the base-image-only resolver on restore, replay, and fork.

```rust
pub struct RuntimeAssets {
    pub base_image_path: PathBuf,
    pub base_image: FileBase,
}

impl ImageResolver {
    pub fn resolve_runtime_assets(
        &self,
        config: &MachineConfig,
    ) -> Result<RuntimeAssets, ImageResolverError> {
        config
            .validate()
            .map_err(ImageResolverError::InvalidConfig)?;
        let (base_image_path, base_image) = self.open_base_image(&config.base_image_hash)?;
        Ok(RuntimeAssets {
            base_image_path,
            base_image,
        })
    }
}
```

Then change the non-boot paths to call it:

```rust
let assets = image_resolver
    .resolve_runtime_assets(&config)
    .map_err(image_error_to_status)?;
let bus = build_bus(&config, assets.base_image, RuntimeVmMem(slot.guest_mem.clone()))?;
```

For `Fork`, apply the same base-image-only resolver where each child bus is built, or introduce a `FileBase` cloning/opening helper if the bus needs a fresh file handle per child.
