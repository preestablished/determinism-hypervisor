# Action Items

## Critical

- [ ] Fix bzImage loading to copy the protected-mode kernel image from `setup_bytes`, not `setup_bytes + payload_offset`.
- [ ] Rename/update layout fields and writer code so "payload" is not confused with the loaded kernel image.

## Important

- [ ] Replace synthetic bzImage tests with fixtures that distinguish startup/decompressor bytes from compressed payload bytes.
- [ ] Extend `linux_entry_smoke` to execute a bounded first `KVM_RUN`/trace when artifacts are present.

## Suggestions

- [ ] Consider writing and loading a concrete Linux boot GDT.
- [ ] Add an upstream-divergence ledger entry for the pv-blk vs virtio-blk contract if upstream docs still differ.
- [ ] Deduplicate the M9 artifact env-var helper before more Linux tests copy it.
