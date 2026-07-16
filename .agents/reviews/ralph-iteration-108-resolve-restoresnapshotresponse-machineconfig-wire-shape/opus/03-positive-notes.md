Good patterns to preserve:

- proto/hypervisor.proto:55 keeps MachineConfig evolution append-only by adding cpuid_table and device_set after the existing fields.
- crates/dh-proto/src/lib.rs:178 pins the new wire shape with byte-level checks for field 11 and packed field 12.
- crates/dh-worker/src/proto_map.rs:80 keeps outbound conversion straightforward and explicit.
- crates/dh-worker/src/proto_map.rs:100 keeps inbound conversion fallible and validates lossy boundaries before returning a domain MachineConfig.
- crates/dh-worker/src/proto_map.rs:353 proves canonical bytes survive a proto round trip while intentionally defaulting non-wire resync_slack.

