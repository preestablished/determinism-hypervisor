Suggestions addressed during review:

- crates/dh-worker/src/proto_map.rs:371
  Add invalid-shape coverage for malformed boot hashes, unspecified and unknown hash_epochs, unsorted cpuid_table, unsorted device_set, oversized device ids, and missing boot.

- crates/dh-proto/src/lib.rs:217
  Add a RestoreSnapshotResponse envelope regression to prove the nested MachineConfig carries cpuid_table and device_set through encode/decode.

