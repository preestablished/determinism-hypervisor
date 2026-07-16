- The resolver returns semantic `ResolvedBoot` values instead of exposing raw proto bytes to future CreateVm code.
- `open_base_image` reuses the verified file descriptor through `FileBase::from_file`.
- The cache key helper avoids adding a hex dependency for a fixed lowercase BLAKE3 filename format.
- The new tests cover happy paths, missing blobs, hash mismatch, oversized blobs, directories, and symlinks.

