# Critical And Important Issues

## Critical

No Critical issues found.

## Important

### Important: failed runtime-table inserts can delete runtimes this transaction did not create

- File: `crates/dh-worker/src/service.rs:373`
- File: `crates/dh-worker/src/service.rs:453`
- File: `crates/dh-worker/src/service.rs:532`

`rollback_lifecycle_leases` begins by calling `runtimes.take()` for every lease slot. That is correct only after the current transaction has successfully inserted those runtimes. The insert failure paths call the same rollback helper even though `runtimes.insert()` and `runtimes.insert_many()` failed before installing any new runtime for this transaction.

On `RuntimeError::Occupied`, the occupied runtime is necessarily pre-existing manager/runtime drift or an unrelated future internal mutation. The rollback then removes and drops that pre-existing runtime while destroying the newly allocated manager lease. In the fork path this is especially surprising because `RuntimeTable::insert_many` is all-or-nothing: if it returns `Occupied`, none of the child runtimes from this transaction were inserted, yet rollback can still take entries from those child slots.

Suggested fix: split manager-only lease rollback from rollback that removes known-inserted runtime slots. Use the manager-only path for build failures and failed `insert`/`insert_many`; use the inserted-runtime path only after a runtime table insert has succeeded.

```rust
fn rollback_manager_leases(
    method: &'static str,
    manager: &SlotManager,
    leases: &[Lease],
    now_ms: u64,
) -> Result<(), Status> {
    let mut errors = Vec::new();
    for lease in leases {
        if let Err(e) = manager.destroy(lease, now_ms) {
            errors.push(format!("slot {}: {e:?}", lease.slot_id));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Status::internal(format!(
            "{method} rollback could not release manager leases: {}",
            errors.join(", ")
        )))
    }
}

fn rollback_inserted_lifecycle_leases(
    method: &'static str,
    manager: &SlotManager,
    runtimes: &WorkerRuntimeTable,
    leases: &[Lease],
    inserted_runtime_slots: &[u64],
    now_ms: u64,
) -> Result<(), Status> {
    for &slot_id in inserted_runtime_slots {
        match runtimes.take(slot_id) {
            Ok(_) | Err(RuntimeError::Empty { .. }) => {}
            Err(e) => {
                return Err(Status::internal(format!(
                    "{method} rollback could not remove inserted runtime slot {slot_id}: {e}"
                )));
            }
        }
    }
    rollback_manager_leases(method, manager, leases, now_ms)
}

// No runtime from this transaction was inserted on this path.
if let Err(e) = runtimes.insert(lease.slot_id, runtime) {
    rollback_manager_leases(
        method,
        manager.as_ref(),
        std::slice::from_ref(&lease),
        now_ms,
    )?;
    return Err(runtime_error_to_status(e));
}

// After this point the helper owns the runtime table entry and may remove it.
if let Err(e) = manager.set_position(&lease, icount, base_snapshot_id, now_ms) {
    rollback_inserted_lifecycle_leases(
        method,
        manager.as_ref(),
        runtimes.as_ref(),
        std::slice::from_ref(&lease),
        &[lease.slot_id],
        now_ms,
    )?;
    return Err(slot_error_to_status(e));
}
```
