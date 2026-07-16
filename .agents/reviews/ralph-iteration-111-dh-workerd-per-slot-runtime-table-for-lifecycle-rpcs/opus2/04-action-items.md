## Action Items

### Critical
- None

### Important
- [ ] [crates/dh-worker/src/service.rs:453] Split manager-only rollback from inserted-runtime rollback so failed `insert`/`insert_many` paths cannot delete pre-existing runtime entries.

### Suggestions
- [ ] [crates/dh-worker/src/service.rs:483] Narrow the fork builder context before rfv wiring so future code cannot mutate arbitrary runtime-table slots.
- [ ] [crates/dh-worker/src/service.rs:914] Add an env-gated strict mode so KVM-backed lifecycle tests fail visibly when CI is expected to provide KVM dirty rings.
- [ ] [crates/dh-worker/src/runtime.rs:82] Use a set for duplicate detection in `RuntimeTable::insert_many`.
- [ ] [crates/dh-worker/src/service.rs:328] Add direct tests for `runtime_error_to_status` code and `ErrorDetail` mapping.
