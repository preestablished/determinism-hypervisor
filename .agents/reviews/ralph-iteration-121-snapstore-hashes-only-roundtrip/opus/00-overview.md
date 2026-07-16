Review: iteration 121, branch `ralph/iteration-121-snapstore-hashes-only-roundtrip`

Scope reviewed:
- `crates/dh-snapshot/Cargo.toml`
- `crates/dh-snapshot/tests/snapstore_readiness.rs`
- `Cargo.lock` dependency delta for `dh-snapshot`

Local verification:
- `cargo test -p dh-snapshot --test snapstore_readiness -- --nocapture`
- Result: passed, 3 tests.

Overall assessment:
- The in-process store fixture is scoped and uses temporary state.
- The `put_pages -> put_snapshot -> resolve_pages -> get_snapshot` happy path is covered through the blocking client and asserts byte-identical manifest fetch.
- The previous Required finding is addressed: `LiveStore::resolve_pages_raw_hashes_only` now uses the generated gRPC client directly and asserts raw `hashes_only=true` responses carry empty payload bytes on the wire.
- Same-snapshot baseline coverage strengthens the `resolve_pages(snapshot_ref, Some(snapshot_ref), true)` planning contract.
- The `MissingPages` live error path is concrete and now verifies a mixed present/missing case: one page is uploaded, `put_snapshot` rejects the manifest, and the typed error lists exactly the absent page hashes with `parent_ref: None`.

Required findings remaining: none.
