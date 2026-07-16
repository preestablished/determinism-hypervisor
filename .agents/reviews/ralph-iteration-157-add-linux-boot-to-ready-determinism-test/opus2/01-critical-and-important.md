# 01-critical-and-important.md

Important: `tests/determinism/tests/linux_ready.rs:73` and `tests/determinism/tests/linux_ready.rs:134` run from source artifact paths after hashes/config are computed. Risk: if `DH_M9_BZIMAGE`, `DH_M9_INITRAMFS`, or `DH_M9_GAME_IMAGE` changes between `populate_m9_image_cache()` and the cold boots, the test can pass two deterministic boots of bytes that do not match `machine_config_hash`. This weakens the artifact/cache oracle and differs from the worker resolver pattern, which opens verified cache blobs. Recommended fix: after hashing, read/open `$DH_M9_IMAGE_CACHE/<lowercase-blake3-hex>` for bzImage, initramfs, and game image, or immediately rehash the exact buffers/file descriptor used for KVM/device setup.

No critical findings found.
