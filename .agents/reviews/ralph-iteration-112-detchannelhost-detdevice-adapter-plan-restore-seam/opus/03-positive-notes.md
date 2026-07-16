# Positive Notes

- `crates/dh-devices/src/detchannel.rs:126` - the adapter is intentionally thin and keeps the live detchannel ABI on PIO while still letting DHSNAP treat EVTC like a bus device section.

- `crates/dh-devices/src/detchannel.rs:175` - the `DetDevice` impl applies `Send + 'static` bounds to the memory handle, fault plan, and plan factory, which matches the `Box<dyn DetDevice>` bus contract.

- `crates/dh-devices/src/detchannel.rs:199` - restore obtains a fresh plan from the factory for each EVTC restore, avoiding stale `FaultPlan` occurrence counters across slot reuse.

- `crates/dh-devices/src/detchannel.rs:974` - the adapter-level bus test covers MAGIC/VERSION service by `MmioBus` and confirms detchannel-specific MMIO offsets are RAZ/WI.

- `crates/dh-worker/tests/restore_engine.rs:232` - the joint KVM test validates the important integration path: a snapshot taken with attached EVTC restores into a fresh slot, reattaches through the existing device loop, re-reads the manifest, and produces matching bus state.

- `crates/dh-worker/tests/common/mod.rs:64` - implementing `detguest_host::GuestMem` for the existing `VmMem` test adapter keeps the restore test using the same `GuestMemoryMmap` backing as the slot.
