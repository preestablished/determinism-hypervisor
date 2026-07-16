# Suggestions

### S1 — The wrong-tenant auto-thaw is defended *only* by `force_destroy` clearing `parent`; pin it with a regression test

**File:** `crates/dh-worker/src/slot_manager.rs:453-491` (`force_destroy`),
`695-707` (`release`)

The prompt's worry — parent force_destroyed → slot id reused by a new tenant →
old child destroyed → `release()` decrements the **new** tenant's
`live_children` and could flip a new `Frozen` parent to `Paused` — is real in
shape but **defended in practice**: `force_destroy` sets `slots[i].parent = None`
on every cascaded child *before* `release(idx)`, so a later `destroy(old_child)`
sees `parent = None` and never touches the reused slot. Good.

But this safety rests entirely on that one `parent = None` line, and there is no
test that exercises the *reuse-then-destroy-orphan* ordering against a new
tenant. A future refactor that moves the orphaning into `release` or drops it
"because the child is faulted anyway" would silently reintroduce the bug.
Suggest a direct regression test:

```rust
#[test]
fn orphaned_child_destroy_never_thaws_a_reused_parent_slot() {
    let m = manager(4, LeasePolicy::default());
    let p = m.allocate(0).unwrap();              // slot 0
    let kids = m.fork(&p, 1, 0).unwrap();        // child in slot 1, parent=Some(0)
    m.force_destroy(p.slot_id).unwrap();         // slot 0 freed, child faulted, parent=None
    // New tenant reuses slot 0 and forks its own children → slot 0 Frozen again.
    let p2 = m.allocate(0).unwrap();
    assert_eq!(p2.slot_id, 0);
    let _kids2 = m.fork(&p2, 1, 0).unwrap();
    assert_eq!(m.slot_info(0).unwrap().state, SlotState::Frozen);
    // Destroy the OLD faulted orphan: must not touch the new tenant in slot 0.
    m.destroy(&kids[0], 0).unwrap();
    assert_eq!(m.slot_info(0).unwrap().state, SlotState::Frozen);
    assert_eq!(m.slot_info(0).unwrap().live_children, 1);
}
```

### S2 — Document the `checkout_write` TOCTOU window and its v1 safety argument

**File:** `crates/dh-worker/src/slot_manager.rs:261-274` (`checkout_write`)

`checkout_write` validates under the lock then **releases it**; the daemon makes
the engine call afterward. Between the two, the housekeeping thread's
`reclaim_expired` could (under a TTL policy) fault or free the slot, and the
engine would then run against a stale snapshot of the slot state. Under v1 this
is safe — `reclaim_expired` is a no-op without a TTL, and the single-orchestrator
contract serializes mutations per slot — but that argument lives only in the
reviewer's head, not the code. The day someone enables `with_ttl`, this becomes
a live race. Add a doc line to `checkout_write` stating the contract: "the
returned Ok is a point-in-time check; the caller holds no lock during the engine
call, so under a TTL policy the daemon must re-validate after any await point or
serialize per slot." This is the natural home for the assumption the rest of the
module quietly relies on.

### S3 — `children as u32` is an unchecked narrowing cast

**File:** `crates/dh-worker/src/slot_manager.rs:582`

`slots[parent_idx].live_children += children as u32;` narrows `usize → u32`. With
a slot count bounded by `physical_cores - 2` this can never overflow in
practice, but it is exactly the silent-`as`-cast shape the module's own
deny-grep test polices for enums. Since `children` is already bounded by the
free-slot count (`free.len() < children` bailed otherwise), a
`u32::try_from(children).expect(...)` or an explicit `debug_assert!` would
document the invariant without changing behavior. Minor.

### S4 — Domain `Lease` derives `Debug`, exposing the token to `{:?}` logging

**File:** `crates/dh-worker/src/slot_manager.rs:46` (`#[derive(... Debug ...)] struct Lease`)

`Lease.token` is the control-plane secret that gates every mutating RPC. With a
derived `Debug`, any `tracing`/`eprintln!("{lease:?}")` at a future call site
prints all 16 bytes into logs. It is a host control-plane token, not a
guest-determinism secret, so this is defense-in-depth rather than a determinism
issue — but a manual `Debug` that redacts the token (`token: [redacted; 16]`)
costs nothing now and removes a footgun before the daemon starts logging leases.
The `PartialEq`/`Eq`/`Clone` derives are all needed and fine.

### S5 — `reclaim_expired`'s "Paused parent mid-thaw" comment overstates the reachable states

**File:** `crates/dh-worker/src/slot_manager.rs:739-741`

The `_ => {}` arm comment says it covers "Frozen (children live) and Paused
parents mid-thaw race." I traced for a `Paused` slot with `live_children > 0`
and could not construct one: `fork` is the only place `live_children` is
incremented and it transitions the parent to `Frozen` atomically under the same
lock, and the auto-thaw in `release` only sets `Paused` *after*
`live_children == 0`. So a `Paused` parent always has `live_children == 0` and is
handled by the first match arm, never the fallthrough. The fallthrough arm is
therefore reachable only for `Frozen`-with-children (and `Empty`, which never has
a lease). The code is correct; the comment describes a state that cannot occur,
which will mislead the next reader into thinking `Paused + live_children > 0` is
a real case to defend. Trim the comment to "Frozen with live children: wait for
the children's own expiry to release them (the last release auto-thaws)."

### S6 — `force_destroy` / `destroy` discard the result of `transition(Empty)`

**File:** `crates/dh-worker/src/slot_manager.rs:658, 674`

`slots[idx].state.transition(SlotState::Empty)?;` is used purely as a validity
gate — the `Ok(Empty)` is dropped and `release()` overwrites the entry with
`empty()`. This is correct and intentional, but the discarded-result pattern
reads like a bug at a glance (`#[must_use]` would normally flag it; here the `?`
consumes it). A one-line `// validate-only: release() sets Empty below` on each
of the two sites would make the intent obvious. Trivial.
