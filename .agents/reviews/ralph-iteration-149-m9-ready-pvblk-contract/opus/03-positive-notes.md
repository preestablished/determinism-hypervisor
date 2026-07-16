# Positive Notes

- `crates/dh-vmm/src/config.rs:100` and `crates/dh-worker/src/proto_map.rs:176` correctly force BzImage cmdline canonicalization before hashing and keep the proto surface as append-only extras.
- `crates/dh-worker/src/image_resolver.rs:156` validates `MachineConfig` before resolving blobs, and `crates/dh-worker/src/image_resolver.rs:214` keeps boot blobs size-capped, regular-file-only, `O_NOFOLLOW`, and BLAKE3-verified before bytes escape.
- `crates/dh-vmm/src/boot/linux_bzimage.rs:606` builds boot params from zeroed memory and has good host-side tests for e820 layout, non-overlap, and byte stability.
- `docs/decisions/m9-linux-ready-and-block-device.md:23` makes the pv-blk `/dev/vdb` and detchannel Ready EventKind 14 contract explicit, which is the right determinism boundary for M9.
