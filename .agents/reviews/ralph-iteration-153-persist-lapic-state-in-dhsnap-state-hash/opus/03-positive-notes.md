# Positive Notes

- `crates/dh-snapshot/src/dhsnap.rs:384` encodes the `LAPC` section field-by-field in little-endian form and reserves explicit padding rather than relying on Rust struct layout.
- `crates/dh-snapshot/src/dhsnap.rs:461` rejects nonzero reserved bytes, and `crates/dh-snapshot/src/dhsnap.rs:491` keeps empty v1 compatibility constrained to reset LAPIC state.
- `crates/dh-vmm/src/lapic.rs:208` validates decoded snapshot semantics before constructing `LocalApic`, including x2APIC, base address, unsupported MSR bits, timer state, and pending ICR delivery.
- `crates/dh-vmm/src/hash.rs:370` frames LAPIC hash input with tag, version, and length, which keeps the new section deterministic and extension-safe.
- `crates/dh-worker/src/restore_engine.rs:306` treats malformed `LAPC` as a loud restore failure while still accepting legacy empty v1 reset snapshots.
- `crates/dh-worker/src/snapshot_engine.rs:347` writes `LAPC` in the fixed DHSNAP order immediately after `VCPU`, and the golden fixture hash `f67536b64965ac7f783a5f2a42993754b0efbb1b8af3b249d7c4d603fa29e367` matches the new fixture.
- `crates/dh-worker/src/service.rs:3725` includes live LAPIC bytes in worker run-control state hashing, so device-side interrupt state participates in epoch and terminal hashes.
- `crates/dh-worker/tests/lapc.rs:84` adds end-to-end coverage for non-reset LAPIC snapshot/restore/resnapshot fixed-point behavior.
