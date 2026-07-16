- crates/dh-worker/src/image_resolver.rs:156 keeps the resolver scoped to CreateVm assets and validates MachineConfig before resolving blobs.
- crates/dh-vmm/src/blkfile.rs:24 now accepts an already-open file, so the descriptor handed to pv-blk is the descriptor the resolver verified.
- .agents/docs/determinism-hypervisor/ARCHITECTURE.md:760 documents the cache installation and immutability contract.

