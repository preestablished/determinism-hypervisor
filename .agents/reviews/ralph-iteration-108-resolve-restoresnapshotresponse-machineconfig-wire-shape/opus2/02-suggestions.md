Suggestions addressed during review:

- crates/dh-proto/src/lib.rs:217
  Add response-level coverage by wrapping the populated MachineConfig in RestoreSnapshotResponse and asserting nested cpuid_table/device_set after decode.

- crates/dh-worker/src/proto_map.rs:371
  Broaden negative coverage around hash_epochs, boot hashes, cpuid_table ordering, device_set ordering, and non-canonical landing fields such as skid_margin.

