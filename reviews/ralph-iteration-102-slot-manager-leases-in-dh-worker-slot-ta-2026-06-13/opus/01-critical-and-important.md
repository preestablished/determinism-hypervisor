# Critical & Important Findings

## Critical

**None.** No correctness, safety, or security defect blocks merge. State-machine
fidelity, lease validation, and the write-path guard are all sound.

## Important

### I1 — `fork` fabricates an `InvalidTransition` value that describes a *legal* transition

**File:** `crates/dh-worker/src/slot_manager.rs:330-336`

```rust
if slots[parent_idx].ram_is_cow {
    // fork_engine: a CoW child's RAM cannot be re-forked.
    return Err(SlotError::State(SlotStateError::InvalidTransition {
        from: slots[parent_idx].state,
        to: SlotState::Frozen,
    }));
}
```

A tier-A CoW child is always `Paused` when this branch fires (children are
registered Paused and `ram_is_cow` is only ever set on them). So the synthesized
error reads `InvalidTransition { from: Paused, to: Frozen }` — but
`Paused → Frozen` **is a legal edge** in `can_transition` (it is exactly the edge
an ordinary fork takes). The refusal reason here is *not* the state machine; it is
the `ram_is_cow` invariant from the fork_engine contract. Emitting a state-machine
error that the state machine would itself accept is actively misleading: anyone
debugging a "fork refused" report will check the transition relation, find the
edge is legal, and be sent down the wrong path. It also means the `fork` CoW-refusal
and a genuine `Paused→Frozen` violation become indistinguishable at the wire
(`FAILED_PRECONDITION` for both, same `State(...)` payload).

The behavior (refusing the fork, all-or-nothing) is correct; only the error
*identity* is wrong. The fix is a dedicated variant so the cause is legible:

```rust
// in SlotError:
/// A tier-A CoW child cannot itself be a fork parent (fork_engine:
/// snapshot → restore into a fresh slot instead). Maps to FAILED_PRECONDITION.
CowChildCannotFork { slot_id: u64 },

// in fork():
if slots[parent_idx].ram_is_cow {
    return Err(SlotError::CowChildCannotFork { slot_id: parent.slot_id });
}
```

The existing test `fork_freezes_parent_accounts_children_and_autothaws` only
asserts `Err(SlotError::State(_))` for this case, so it would need to match the
new variant — a one-line test update that also makes the test *prove* the right
reason rather than accepting any state error.

**Severity rationale:** Important, not Critical — it is a diagnostics defect on a
control-plane error path, not a state corruption or a missed gate. Flagged because
this module's entire value proposition is "the state machine has exactly one
home"; a hand-rolled state-machine error that contradicts that home undercuts the
property reviewers (and operators) will rely on.
