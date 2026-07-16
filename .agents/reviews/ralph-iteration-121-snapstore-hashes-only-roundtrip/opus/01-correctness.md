Correctness notes:

- `spawn_live_store` creates a fresh `TempDir`, binds an in-process snapstore server on a UDS path under that temp root, and connects the blocking client to that UDS path. This avoids shared-state contamination between tests.
- `live_snapstore_roundtrip_pins_hashes_only_contract` uses full 4096-byte pages, uploads them first, stores a manifest container, verifies the returned `SnapshotRef`, fetches the same container byte-for-byte, and resolves payload mode before hashes-only mode.
- The expected page ordering is stable even though `sample_pages` is intentionally out of order: `Manifest::new_full` sorts entries and requires contiguous full-manifest page indices, while `expected_pages` sorts by page index before comparison.
- `LiveStore::resolve_pages_raw_hashes_only` connects through the generated gRPC client and consumes raw stream messages, so the wire-level payload assertion is not masked by the high-level client's local `hashes_only` mapping.
- `live_snapstore_missing_pages_error_is_typed_and_complete` seeds only page index 1, then asserts the store reports the two absent hashes for indices 0 and 2. That is a stronger completeness check than an all-pages-missing setup.

No direct data-corruption or wrong-API-call bug was found in the reviewed code.
