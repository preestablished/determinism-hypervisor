Critical:

- No critical findings.

Important:

- crates/dh-worker/src/image_resolver.rs:181
  Boot blob reads were unbounded before `read_to_end`, so a valid cached hash for a huge file could force excessive allocation.
  Status: fixed with `MAX_KERNEL_BYTES`, `MAX_INITRAMFS_BYTES`, `TooLarge`, and cap tests at crates/dh-worker/src/image_resolver.rs:16 and crates/dh-worker/src/image_resolver.rs:531.

- crates/dh-worker/src/image_resolver.rs:180
  Boot blobs were hashed in one pass and then read in a second pass, so mutable cache files could return bytes different from those verified.
  Status: fixed by hashing the exact returned buffer in `read_blob_limited` at crates/dh-worker/src/image_resolver.rs:214.

