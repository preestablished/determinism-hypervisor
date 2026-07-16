# Critical And Important

## Critical

No critical issues found.

## Important

### I1 - Restored pending injects lose name resolution for name-specific fault plans

**Severity:** Important
**File:** `crates/dh-devices/src/detchannel.rs:312`

**Problem:** EVTC v2 serializes each pending `InjectQuery` as only `iseq u32 | name_id u32`. During restore, `Channel::attach` creates a fresh channel with an empty intern cache. The restored-answer path then calls `channel.intern_name(name_id)` at `crates/dh-devices/src/detchannel.rs:667`, which returns `None` for a query whose `NameIntern` was already drained before the snapshot. That changes the decision made by any name-specific `FaultPlan`: uninterrupted execution would call `decide(..., Some("disk_fault"))`, while restored execution calls `decide(..., None)`. The added test uses `name_glob: "*"`, so it does not catch this divergence.

**Suggested fix snippet:**

Persist the resolved pending name, or enough intern-table state for the pending `name_id`s, alongside each restored pending query. If EVTC v2 is not yet frozen, extend v2; otherwise bump again.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingInjectSnapshot {
    name_id: u32,
    name: Option<String>,
}

// During drain, after Channel has folded NameIntern records into its cache.
if let OwnedPayload::InjectQuery { iseq, name_id } = &ev.payload {
    let name = channel.intern_name(*name_id).map(str::to_owned);
    self.pending_injects.insert(
        *iseq,
        PendingInjectSnapshot {
            name_id: *name_id,
            name,
        },
    );
}

// During restored answer, prefer any live intern discovered after restore,
// then fall back to the serialized name.
if let Some(pending) = self.restored_pending_injects.remove(&iseq) {
    self.pending_injects.remove(&iseq);
    let live_name = channel.intern_name(pending.name_id).map(str::to_owned);
    let name = live_name.as_deref().or(pending.name.as_deref());
    let value = self
        .responder
        .plan_mut()
        .decide(iseq, pending.name_id, name)
        .pack();
    let mut sink = CtxSink { ctx };
    sink.pio_answer(PORT_INJECT, value);
    return value;
}
```

Add a regression test that drains `NameIntern { name_id: 11, name: b"disk_fault" }` plus `InjectQuery { iseq: 7, name_id: 11 }`, snapshots before `IN PORT_INJECT`, restores with a `TableFaultPlan` matching `name_glob: "disk_fault"`, and asserts the restored `IN` returns the same nonzero decision as the uninterrupted path.
