Critical:

- No critical findings.

Important:

- crates/dh-worker/src/image_resolver.rs:181
  `read_blob` used unbounded `read_to_end` and could allocate a huge kernel/initramfs blob selected by hash.
  Status: fixed with explicit boot-blob caps, `TooLarge`, and tests.

- crates/dh-worker/src/image_resolver.rs:193
  Regular-file validation happened after `File::open`, which followed symlinks and could block on special files.
  Status: fixed by using `O_NOFOLLOW | O_NONBLOCK`, fd metadata validation, and symlink/directory tests at crates/dh-worker/src/image_resolver.rs:298 and crates/dh-worker/src/image_resolver.rs:565.

