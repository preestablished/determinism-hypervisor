Suggestions:

- crates/dh-worker/src/proto_map.rs:104
  Document that count range validation is owned by slot-manager capacity checks before service wiring calls the helper.
  Status: done.

- crates/dh-worker/src/proto_map.rs:532
  Add a 33-byte bad-seed test alongside the 31-byte bad-seed case.
  Status: done.

