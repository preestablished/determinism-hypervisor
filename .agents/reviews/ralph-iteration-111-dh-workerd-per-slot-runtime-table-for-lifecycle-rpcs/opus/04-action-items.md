## Action Items

### Critical
- None

### Important
- [ ] [crates/dh-worker/src/service.rs:491] Validate fork lease/state through `SlotManager` before consulting the runtime table.
- [ ] [crates/dh-worker/src/service.rs:436] Stop reusing the lifecycle start timestamp for post-build publication under TTL-enabled lease policies.

### Suggestions
- [ ] [crates/dh-worker/src/runtime.rs:82] Use a `HashSet` or equivalent linear duplicate check in `insert_many`.
- [ ] [crates/dh-worker/src/service.rs:366] Refactor rollback error handling so original engine failures and rollback failures are both preserved.
- [ ] [crates/dh-worker/src/service.rs:483] Narrow or document the `build_runtimes` runtime-table authority contract.
- [ ] [crates/dh-worker/src/service.rs:329] Add tests pinning `runtime_error_to_status` detail codes.
