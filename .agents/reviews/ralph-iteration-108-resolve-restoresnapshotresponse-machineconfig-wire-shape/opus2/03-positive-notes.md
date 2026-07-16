Good patterns to preserve:

- crates/dh-worker/src/proto_map.rs:41 gives MachineConfig wire conversion errors concrete variants, which keeps boundary failures inspectable in tests.
- crates/dh-worker/src/proto_map.rs:121 rejects proto enum zero and unknown hash_epochs values rather than silently defaulting.
- crates/dh-worker/src/proto_map.rs:135 checks device ids fit the domain u16 width before constructing MachineConfig.
- crates/dh-worker/src/proto_map.rs:155 runs MachineConfig::validate() after reconstruction, so domain invariants remain owned by dh-vmm.

