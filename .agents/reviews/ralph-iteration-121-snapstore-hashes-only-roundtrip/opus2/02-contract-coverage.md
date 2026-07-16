# Contract Coverage

Covered contracts:
- Compile-time pins still cover the client surface and key method signatures.
- The variant pin at `crates/dh-snapshot/tests/snapstore_readiness.rs:91` covers the current `ClientError` enum fields, including `MissingPages { page_hashes, parent_ref }`.
- The live round-trip at `crates/dh-snapshot/tests/snapstore_readiness.rs:307` exercises pages-first ingest, manifest commit, full resolve, hashes-only resolve, raw hashes-only wire resolve, same-snapshot baseline resolve, and byte-identical snapshot fetch.
- The blocking-client hashes-only assertion at `crates/dh-snapshot/tests/snapstore_readiness.rs:345` verifies that planning mode omits payloads while preserving page index/hash results.
- The raw generated gRPC assertion at `crates/dh-snapshot/tests/snapstore_readiness.rs:358` verifies that hashes-only mode omits payload bytes on the wire, not just after client facade decoding.
- The same-snapshot baseline assertions at `crates/dh-snapshot/tests/snapstore_readiness.rs:371` and `crates/dh-snapshot/tests/snapstore_readiness.rs:379` pin empty deltas through both blocking and raw clients.
- The mixed missing-pages test at `crates/dh-snapshot/tests/snapstore_readiness.rs:387` verifies that the client decodes the server detail into `ClientError::MissingPages`, treats it as non-retryable, excludes the uploaded page, includes every absent hash, preserves expected manifest/index order, and reports no missing parent for a full manifest.

Residual note:
- A child-delta-versus-parent `baseline_ref` case is not covered here. Same-snapshot baseline coverage satisfies the previous minimal gap and the bead's stated follow-up scope; a true ancestor/delta baseline test would be useful if `dh-snapshot` begins depending on changed-page-only restore planning semantics.
