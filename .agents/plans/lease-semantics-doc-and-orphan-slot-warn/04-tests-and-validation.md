# Tests And Validation

## Focused Unit Tests

Add tests in `crates/dh-worker/src/service.rs` beside the existing slot status
mapping and slot-manager service tests. Drive the shared sink-injected
manager-aware emission/status core used by the production stderr adapter; a
classifier-only test is not sufficient. Do not redirect global stderr.

Required tests:

1. **Emits on the signature.** Fill a nonempty manager, leave every slot
   `Paused`, set one shared icount, and assign at least one known
   `base_snapshot_id`. Use distinct byte patterns for test tokens and base ids.
   Pass `NoFreeSlot`. Assert exactly one sink call and assert
   the line includes `WARN:`, `possible orphaned slots`, every slot id, the
   shared icount, full base id/`none`, and `rom-operator-bridge-72o`. Assert it
   does not contain the distinct known test lease token encoding. Assert the
   same helper returns `Code::ResourceExhausted`.
2. **Silent for differing icounts.** Fill the manager with paused slots at two
   different icounts. Pass `NoFreeSlot` and assert zero sink calls.
3. **Silent unless every slot is paused.** Use table-driven cases with one
   `Running` row and, if concise to construct, one `Frozen` row. Pass
   `NoFreeSlot` and assert zero sink calls.

Recommended small edge tests:

- empty row slice is silent (guards against vacuous `all`);
- a non-`NoFreeSlot` error is silent even for uniform paused rows;
- differing base snapshot ids still warn and are rendered per slot, because
  base equality is payload context rather than part of the requested signal.

Preserve the existing `slot_errors_map_to_api_status_classes` test. The shared
manager-aware helper test must prove that emission leaves `NoFreeSlot` mapped to
`Code::ResourceExhausted`.

## Focused Commands

Run formatting first, then the focused library tests:

```bash
cargo fmt --all -- --check
cargo test -p dh-worker --lib possible_orphan -- --nocapture
cargo test -p dh-worker --lib slot_errors_map_to_api_status_classes
cargo test -p dh-worker --lib slot_manager::tests
```

Choose test names containing `possible_orphan` so the filter is stable. If the
tests are named differently, record the exact equivalent commands in the
request resolution.

## Workspace Gates

The change touches a central service file even though production behavior is
log-only. Run:

```bash
cargo test -p dh-worker --lib
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

If a pre-existing ignored hardware gate is not selected, do not broaden this
change by running it. If a workspace failure is unrelated, capture its exact
command/output, file a Beads follow-up when needed, and do not describe the
suite as passing.

## Documentation Audit

Manually verify each statement in both owner docs against current source:

- `LeasePolicy::default` and `with_ttl`;
- token validation and status mapping;
- `renew` and lack of a public renew RPC;
- `reclaim_expired` child release/parent thaw/single-pass behavior;
- explicit `now_ms` injection;
- absence of a production caller and disconnect hook;
- active `DestroyVm` path;
- all manager-aware `NoFreeSlot` wrappers.

Run these negative searches and include the result in review notes:

```bash
rg -n "reclaim_expired" crates/dh-worker/src
rg -n "LeasePolicy::with_ttl" crates/dh-worker/src
rg -n "RenewLease|reclaim_session|disconnect" proto crates/dh-worker/src
```

Expected at plan time: production has no reaper call, production uses no
`with_ttl`, and there is no public renewal/disconnect API.

Audit the final allocation mappings explicitly:

```bash
rg -n "\.(allocate|check_fork|fork)\(" crates/dh-worker/src/service.rs
rg -n "allocation_error_to_status" crates/dh-worker/src/service.rs
```

The audit must show one shared Create/Restore allocation seam, both Fork
check/commit seams, and the direct `VerifyReplay` allocation seam using the
production adapter. Record the resolved line numbers in `04-resolution.md`.
