# Positive Notes

- The adoption is centralized in `common::spawn_store_at`, which means existing worker integration tests start exercising the production-like fast path without one-off per-test rewrites.
- The corrupt page-channel helper is a good acceptance probe because it fails only when the page-channel integrity path is active and correctly surfaced as `ClientError::BatchBlake3Mismatch`.
- The existing 32 MiB regression remains valuable after this change: it now covers the large `put_snapshot_from_parts` path through the updated fixture rather than just the old pure-gRPC client shape.
- The snapshot readiness change is appropriately limited to documentation/API pin wording. It keeps compile-time surface pins in `dh-snapshot` while assigning live behavior coverage to `dh-worker`, where the real store fixture exists.
- Socket names remain caller supplied, preserving the durability test's ability to restart over the same data root without depending on reuse of an old UDS path.
