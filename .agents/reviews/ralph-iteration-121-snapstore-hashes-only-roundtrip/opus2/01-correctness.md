# Correctness

No Required correctness issues found in the scoped changes.

The round-trip test uses the sibling store through a real in-process server fixture and verifies:
- `put_pages` accepts the concrete page payloads and reports all pages as new.
- `put_snapshot` returns the content-addressed `SnapshotRef` computed from the manifest container.
- `get_snapshot` returns byte-identical manifest bytes.
- `resolve_pages(..., hashes_only=false)` returns page index, hash, and payload for every page.
- `resolve_pages(..., hashes_only=true)` returns the same page index/hash sequence and omits payloads through the blocking client facade.
- Raw generated gRPC `ResolvePages` also returns empty `payload` bytes in `hashes_only=true` mode.
- Same-snapshot `baseline_ref` returns an empty page delta through both client layers.

The prior ordering concern is resolved. `sample_pages()` is intentionally unsorted at `crates/dh-snapshot/tests/snapstore_readiness.rs:246`, and `expected_pages()` sorts by page index at `crates/dh-snapshot/tests/snapstore_readiness.rs:250`. The resolve assertions at `crates/dh-snapshot/tests/snapstore_readiness.rs:337` and `crates/dh-snapshot/tests/snapstore_readiness.rs:360` now prove ascending/index order rather than upload/input order.

The prior `MissingPages` completeness concern is resolved. The test seeds only page index 1 at `crates/dh-snapshot/tests/snapstore_readiness.rs:391`, then asserts the returned `ClientError::MissingPages` contains exactly the absent page hashes and `parent_ref == None` at `crates/dh-snapshot/tests/snapstore_readiness.rs:403` and `crates/dh-snapshot/tests/snapstore_readiness.rs:414`.

The fixture lifecycle remains acceptable: `LiveStore::drop` sends `ServerHandle::shutdown()` at `crates/dh-snapshot/tests/snapstore_readiness.rs:174`, and `_dir` keeps the UDS path/data root alive until after the test. The raw generated gRPC helper opens a short-lived channel per check and drains the stream before the fixture is dropped.
