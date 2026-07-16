# Critical And Important Issues

## Important: Fork validates runtime-table state before the slot-manager lease/state authority

- Severity: Important
- File: `crates/dh-worker/src/service.rs:491`

`install_forked_runtimes` calls `runtimes.ensure_occupied(parent.slot_id)` before `manager.fork(&parent, ...)`. That reverses the invariant documented in `runtime.rs`: mutating RPCs should check lease/state through `SlotManager` before consulting the daemon resource table. With an empty, reclaimed, expired, or stale parent slot, this can report `runtime_missing` instead of the authoritative slot-manager error (`stale_lease`, `lease_expired`, `zero_child_fork`, `cow_child_cannot_fork`, etc.). It also makes future lifecycle behavior depend on resource-table state before the state machine has accepted the request.

Suggested fix: add a non-mutating fork precheck to `SlotManager`, use it before the runtime-table check, then call the existing mutating `fork`.

```rust
// slot_manager.rs
pub fn check_fork(
    &self,
    parent: &Lease,
    children: usize,
    now_ms: u64,
) -> Result<(), SlotError> {
    let slots = self.slots.lock().expect("slot table poisoned");
    let parent_idx = parent.slot_id as usize;
    let entry = slots
        .get(parent_idx)
        .ok_or(SlotError::NoSuchSlot(parent.slot_id))?;

    Self::validate_entry(entry, parent, now_ms)?;
    if children == 0 {
        return Err(SlotError::ZeroChildFork {
            slot_id: parent.slot_id,
        });
    }
    if entry.ram_is_cow {
        return Err(SlotError::CowChildCannotFork {
            slot_id: parent.slot_id,
        });
    }
    entry.state.transition(SlotState::Frozen)?;

    let free = slots
        .iter()
        .filter(|slot| slot.state == SlotState::Empty)
        .take(children)
        .count();
    if free < children {
        return Err(SlotError::NoFreeSlot);
    }
    Ok(())
}

// service.rs
manager
    .check_fork(&parent, count, now_ms)
    .map_err(slot_error_to_status)?;
runtimes
    .ensure_occupied(parent.slot_id)
    .map_err(runtime_error_to_status)?;
let child_leases = manager
    .fork(&parent, count, now_ms)
    .map_err(slot_error_to_status)?;
```

## Important: Lifecycle helpers reuse a stale timestamp across long blocking builds

- Severity: Important
- File: `crates/dh-worker/src/service.rs:436`, `crates/dh-worker/src/service.rs:490`

`install_allocated_runtime` and `install_forked_runtimes` capture `now_ms` once, then run arbitrary runtime construction before `set_position` and rollback calls. Under `LeasePolicy::with_ttl`, a build can outlive the TTL, but the later `set_position` still validates with the earlier timestamp. That can publish and return a lease that is already expired in real time, and it bypasses the expiry behavior the slot manager is designed to enforce. Rollback cleanup may deliberately need the original timestamp, but successful publication should not use it.

Suggested fix: separate the lifecycle start timestamp from the publication timestamp, and renew or publish leases at the response boundary. If a lease expired during construction, fail the lifecycle RPC and use the original timestamp only for best-effort cleanup of resources the worker just allocated.

```rust
let allocated_at_ms = lease_now_ms();
let lease = manager
    .allocate(allocated_at_ms)
    .map_err(slot_error_to_status)?;

let runtime = match build_runtime(lease.clone()) {
    Ok(runtime) => runtime,
    Err(e) => {
        rollback_lifecycle_leases(
            method,
            manager.as_ref(),
            runtimes.as_ref(),
            std::slice::from_ref(&lease),
            allocated_at_ms,
        )?;
        return Err(e);
    }
};

let publish_ms = lease_now_ms();
if let Err(e) = manager.renew(&lease, publish_ms) {
    rollback_lifecycle_leases(
        method,
        manager.as_ref(),
        runtimes.as_ref(),
        std::slice::from_ref(&lease),
        allocated_at_ms,
    )?;
    return Err(slot_error_to_status(e));
}

let (icount, base_snapshot_id) = runtime_position(&runtime);
if let Err(e) = runtimes.insert(lease.slot_id, runtime) {
    rollback_lifecycle_leases(
        method,
        manager.as_ref(),
        runtimes.as_ref(),
        std::slice::from_ref(&lease),
        allocated_at_ms,
    )?;
    return Err(runtime_error_to_status(e));
}
if let Err(e) = manager.set_position(&lease, icount, base_snapshot_id, publish_ms) {
    rollback_lifecycle_leases(
        method,
        manager.as_ref(),
        runtimes.as_ref(),
        std::slice::from_ref(&lease),
        allocated_at_ms,
    )?;
    return Err(slot_error_to_status(e));
}
```

For fork, apply the same pattern to each returned child lease before publishing child runtimes. Longer term, a `SlotManager` lifecycle transaction API that starts child lease TTL at successful publication would be cleaner than relying on `renew`.
