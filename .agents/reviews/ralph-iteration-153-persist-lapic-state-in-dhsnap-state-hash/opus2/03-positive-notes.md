# Positive Notes

- `crates/dh-snapshot/src/dhsnap.rs:384` keeps `LapcSection` serialization explicit and little-endian field by field, with reserved-byte validation on decode. That is the right shape for a stable binary fixture format.
- `crates/dh-vmm/src/hash.rs:422` frames LAPC under its own `LAPC` device-section tag before hashing, so device-hash extension remains ordered and domain separated.
- `crates/dh-vmm/src/runctl.rs:235` keeps `hash_device_sections` as a late-bound callback, and the production callers evaluate it at epoch/final hash time after exit handling has updated device state.
- `crates/dh-worker/src/service.rs:3669` uses `capture_bisection_checkpoint_snapshot_with_lapic` in the service recording checkpoint path, keeping recorded bisection checkpoints aligned with the runtime LAPC state.
- `crates/dh-worker/src/restore_engine.rs:306` rejects malformed LAPC sections through `LocalApic::from_lapc_section`, so bad APIC base, x2APIC state, timer, and ICR fixtures fail during restore instead of silently normalizing.
- `crates/dh-worker/tests/lapc.rs:84` adds focused integration coverage for snapshot/restore, malformed LAPC restore, fork inheritance, and snapshot-compare LAPC diffs.
- `crates/dh-snapshot/tests/golden.rs:154` adds a new LAPC v2 kitchen-sink fixture while still pinning the old legacy fixture hash, preserving backward compatibility coverage.
