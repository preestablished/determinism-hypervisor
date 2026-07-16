- `crates/dh-vmm/src/config.rs:100`: BzImage cmdline extras are normalized before hashing, with narrow allowed tokens and baseline override rejection.

- `crates/dh-worker/src/proto_map.rs:176`: Incoming proto BzImage cmdline bytes are treated as extras and canonicalized into the `MachineConfig`, while outgoing proto strips the forced baseline.

- `crates/dh-worker/src/image_resolver.rs:203`: Kernel and initramfs blobs are read into memory only after regular-file checks, size caps, and BLAKE3 verification.

- `docs/decisions/m9-linux-ready-and-block-device.md:23`: The READY and pv-blk contract is explicit about using existing deterministic pv-blk and EventKind 14 rather than serial-only readiness.
