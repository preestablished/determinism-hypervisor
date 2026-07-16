## Action Items

### Critical
No critical items.

### Important
- [ ] [crates/dh-devices/src/detchannel.rs:312] Persist restored pending inject name context, or equivalent intern-table state for pending `name_id`s, so name-specific `FaultPlan` decisions remain identical across OUT/restore/IN.

### Suggestions
- [ ] [crates/dh-devices/src/detchannel.rs:365] Add negative tests for malformed EVTC v2 pending tables.
- [ ] [crates/dh-worker/tests/linux_worker_api.rs:837] Use checked arithmetic when validating v2 pending-table length in the worker test helper.
- [ ] [crates/dh-devices/src/detchannel.rs:312] Replace the silent pending-count `as u32` cast with an explicit checked conversion or documented assertion.
