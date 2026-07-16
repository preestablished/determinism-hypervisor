# Suggestions

### S1 — `reset_slot_dirty_tracking` doc is accurate; tighten one sentence so it can't be misread later

**File:** `crates/dh-worker/src/slot_manager.rs:586-600`

The helper genuinely discharges the restore_engine precondition for the part it
owns: it maps the ring, harvests every entry into a throwaway `DirtyPageSet`, and
`harvest_at_boundary` then calls `reset_dirty_rings(vm)` VM-wide (verified in
`dh-vmm/src/dirty.rs:208-220`). The live test proves a second call returns 0. So
the doc does **not** overclaim — but it says "a previously-run slot is safe to
restore into," and restore_engine's own precondition (`restore_engine.rs:104-110`)
warns the host-side RAM writes *bypass KVM dirty tracking entirely*. The ring
reset is exactly the right and sufficient action (those host-side writes are what
*re-poison* a stale slot, and clearing the ring is what makes them harmless on the
next harvest), but the two-clause relationship is subtle. Consider one clarifying
line so a future reader doesn't conclude this helper somehow also accounts the
bypassing writes:

```
/// ... Returns the number of stale entries discarded. This resets KVM's
/// dirty ring; it does NOT (and need not) account restore_engine's
/// host-side RAM writes — those bypass the ring, which is precisely why a
/// stale ring must be drained before the next incremental snapshot.
```

Documentation only; no behavior change.

### S2 — `reclaim_expired` cross-sweep parent thaw deserves an explicit assertion in the live/unit comment

**File:** `crates/dh-worker/src/slot_manager.rs:503-530`

The two-sweep behavior (a Frozen parent only becomes reclaimable on the sweep
*after* its last child frees, because the loop is single-pass ascending and the
parent typically precedes its children in index order) is correct and tested
(`reclaim_faults_running_slots_and_waits_for_frozen_parents`). But the
correctness depends on the loop being single-pass — if a future edit wrapped it in
a fixpoint loop, a Running slot would fault then immediately free in one sweep,
silently changing the "next sweep frees it" contract the comment at lines 517-520
promises. A one-line invariant note ("this sweep is deliberately single-pass; the
Running→Faulted→free and Frozen-thaw→free handoffs both rely on it") would protect
the property against a well-meaning refactor.

### S3 — `fork`'s `children: usize` → `live_children: u32` accumulation can wrap on absurd inputs

**File:** `crates/dh-worker/src/slot_manager.rs:367`

```rust
slots[parent_idx].live_children += children as u32;
```

`children as usize` is bounded in practice by the free-slot count (a few), so this
cannot realistically overflow, and the `free.len() < children` guard above caps it
at the table size. It is defensively fine. If you want belt-and-suspenders against
a future caller passing a wild `children`, `saturating_add` mirrors the
`saturating_sub` already used in `release()` and keeps the two sides symmetric.
Cosmetic.

### S4 — Two `transition(Empty)?` calls validate-then-discard; a comment would clarify intent

**File:** `crates/dh-worker/src/slot_manager.rs:443` and `:459`

```rust
slots[idx].state.transition(SlotState::Empty)?;   // result discarded
Self::release(&mut slots, idx);                   // sets entry to empty()
```

The `transition(Empty)?` here is used purely as a *gate* (reject Running slots;
the `?` propagates the error), and `release` then resets the entry directly
rather than using the returned `Empty`. This is correct but reads oddly — a
discarded `transition` result usually signals a bug. A trailing comment
(`// gate only: release() resets the entry; Running slots are refused here`) makes
the intent obvious and prevents a future reader from "fixing" it into
`slots[idx].state = slots[idx].state.transition(Empty)?;`.

### S5 — `urandom_token` panics if `/dev/urandom` is unreadable; fine for v1, note the seam exists

**File:** `crates/dh-worker/src/slot_manager.rs:565-572`

The `expect("/dev/urandom must be readable on a Linux worker host")` is acceptable
(a worker host without urandom is unbootable, and token generation has no graceful
degradation). The `with_token_source` seam already exists for tests and a future
audit mode. No change needed; flagging only so the panic is a *known* one, not an
overlooked one. If lease minting ever moves onto a request path where a panic
would take down the daemon, revisit — but for v1 grant-at-allocate it is correct.

### S6 — `default_slot_count` / `parse_core_list` live in `slot_manager` but read like preflight config

**File:** `crates/dh-worker/src/slot_manager.rs:160-187`

These two free functions (core-list parsing, ARCH §9 default slot count) are
config-plumbing the doc itself ties to `preflight::SLOT_CORES`. They are fine
here, but if a `preflight` module owns the cpuset/core story elsewhere, consider
whether these belong next to it to avoid two homes for "how many slots / which
cores." Non-blocking; depends on where the daemon (rfv) ends up reading config.
