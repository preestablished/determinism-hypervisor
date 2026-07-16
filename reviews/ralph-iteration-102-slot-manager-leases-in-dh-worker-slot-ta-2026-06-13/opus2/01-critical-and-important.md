# Critical & Important Findings

## Critical

None. The lease gate, all-or-nothing fork, and force-destroy cascade are
correct, and no path validated by my trace strands a slot or leaks a parent link
into a reused tenant.

---

## Important

### I1 — `fork(parent, children = 0)` freezes the parent with no non-destroy exit

**File:** `crates/dh-worker/src/slot_manager.rs:533-584` (`fork`)

**Problem.** `fork` has no guard against `children == 0`. Trace with `children = 0`:

1. `validate_entry` passes.
2. `ram_is_cow` check passes (Paused parent).
3. `let frozen = ...transition(SlotState::Frozen)?` — succeeds.
4. `free` collects `.take(0)` → empty vec.
5. `if free.len() (0) < children (0)` → `0 < 0` is **false**, so we do NOT bail.
6. `slots[parent_idx].state = frozen;` — the parent is now **Frozen**.
7. The `for idx in free` loop body never runs (no children, no leases).
8. `live_children += 0`.
9. Returns `Ok(vec![])`.

The parent is now `Frozen` with `live_children == 0` and **zero children that
will ever call `release()`**. The only edges out of `Frozen` are:

- `Frozen → Paused` — fired *only* by `release()` on a last-child destroy, and
  there are no children to destroy.
- `Frozen → Empty` — `destroy()` succeeds (`live_children == 0`), so the slot
  *can* be reclaimed.

So a zero-child fork **irreversibly freezes a live slot**: it can no longer run
(`ensure_write_path` denies Frozen), cannot be paused (no `Frozen → Paused`
public path), and cannot be re-forked — the orchestrator's only recourse is to
throw the slot away via `DestroyVm`. That is silent data loss of a prepared VM
from what looks like a successful no-op RPC. Whether the daemon ever passes 0
depends on upstream RPC validation that does not exist yet; the module owns the
state machine and should refuse this at the boundary.

**Fix.** Reject a zero-child fork before any mutation (a fork of nothing is a
caller error, not a freeze):

```rust
pub fn fork(&self, parent: &Lease, children: usize, now_ms: u64) -> Result<Vec<Lease>, SlotError> {
    if children == 0 {
        return Err(SlotError::NoFreeSlot); // or a dedicated SlotError::EmptyFork
    }
    let mut slots = self.slots.lock().expect("slot table poisoned");
    // ... unchanged
}
```

A dedicated variant (`SlotError::EmptyFork`) reads better than reusing
`NoFreeSlot`, but either keeps the parent Paused. Add a unit test:

```rust
#[test]
fn zero_child_fork_does_not_freeze_the_parent() {
    let m = manager(2, LeasePolicy::default());
    let parent = m.allocate(0).unwrap();
    assert!(m.fork(&parent, 0, 0).is_err());
    assert_eq!(m.slot_info(parent.slot_id).unwrap().state, SlotState::Paused);
}
```
